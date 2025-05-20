//! This module determines the fees to pay in txs containing blocks submitted to the L1.

use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use tokio::sync::watch;
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::EthFeeInterface;
use zksync_types::{
    commitment::L1BatchCommitmentMode, pubdata_da::PubdataSendingMode, L1_GAS_PER_PUBDATA_BYTE,
    U256,
};
use zksync_web3_decl::client::{DynClient, L1, L2};

use self::metrics::METRICS;
use super::TxParamsProvider;

mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct GasAdjusterClient {
    inner: Box<dyn EthFeeInterface>,
}

impl From<Box<DynClient<L1>>> for GasAdjusterClient {
    fn from(inner: Box<DynClient<L1>>) -> Self {
        Self {
            inner: Box::new(inner.for_component("gas_adjuster")),
        }
    }
}

impl From<Box<DynClient<L2>>> for GasAdjusterClient {
    fn from(inner: Box<DynClient<L2>>) -> Self {
        Self {
            inner: Box::new(inner.for_component("gas_adjuster")),
        }
    }
}

/// This component keeps track of the median `base_fee` from the last `max_base_fee_samples` blocks.
///
/// It also tracks the median `blob_base_fee` from the last `max_blob_base_fee_sample` blocks.
/// It is used to adjust the base_fee of transactions sent to L1.
#[derive(Debug)]
pub struct GasAdjuster {
    pub(super) base_fee_statistics: GasStatistics<u64>,
    // Type for blob base fee is chosen to be `U256`.
    // In practice, it's very unlikely to overflow `u64` (if `blob_base_fee_statistics` = 10 ^ 18, then price for one blob is 2 ^ 17 ETH).
    // But it's still possible and code shouldn't panic if that happens. One more argument is that geth uses big int type for blob prices.
    //
    // Note, that for L2-based chains it will contains only zeroes.
    pub(super) blob_base_fee_statistics: GasStatistics<U256>,
    // Note, that for L1-based chains the following field contains only zeroes.
    pub(super) l2_pubdata_price_statistics: GasStatistics<U256>,
    // Note, that for L1-based chains the following field contains only zeroes.
    pub(super) gas_per_pubdata_price_statistic: GasStatistics<u64>,

    pub(super) config: GasAdjusterConfig,
    pubdata_sending_mode: PubdataSendingMode,
    client: GasAdjusterClient,
    commitment_mode: L1BatchCommitmentMode,
}

impl GasAdjuster {
    pub async fn new(
        client: GasAdjusterClient,
        config: GasAdjusterConfig,
        pubdata_sending_mode: PubdataSendingMode,
        commitment_mode: L1BatchCommitmentMode,
    ) -> anyhow::Result<Self> {
        // Subtracting 1 from the "latest" block number to prevent errors in case
        // the info about the latest block is not yet present on the node.
        // This sometimes happens on Infura.
        let current_block = client
            .inner
            .block_number()
            .await?
            .as_usize()
            .saturating_sub(1);
        let fee_history = client
            .inner
            .base_fee_history(current_block, config.max_base_fee_samples)
            .await?;

        let base_fee_statistics = GasStatistics::new(
            config.max_base_fee_samples,
            current_block,
            fee_history.iter().map(|fee| fee.base_fee_per_gas),
        );

        let blob_base_fee_statistics = GasStatistics::new(
            config.num_samples_for_blob_base_fee_estimate,
            current_block,
            fee_history.iter().map(|fee| fee.base_fee_per_blob_gas),
        );

        let l2_pubdata_price_statistics = GasStatistics::new(
            config.num_samples_for_blob_base_fee_estimate,
            current_block,
            fee_history.iter().map(|fee| fee.l2_pubdata_price),
        );

        let gas_per_pubdata_price_statistic = GasStatistics::new(
            config.num_samples_for_blob_base_fee_estimate,
            current_block,
            fee_history
                .iter()
                .map(|base_fee| base_fee.gas_per_pubdata()),
        );

        Ok(Self {
            base_fee_statistics,
            blob_base_fee_statistics,
            l2_pubdata_price_statistics,
            gas_per_pubdata_price_statistic,
            config,
            pubdata_sending_mode,
            client,
            commitment_mode,
        })
    }

    /// Performs an actualization routine for `GasAdjuster`.
    /// This method is intended to be invoked periodically.
    pub async fn keep_updated(&self) -> anyhow::Result<()> {
        // Subtracting 1 from the "latest" block number to prevent errors in case
        // the info about the latest block is not yet present on the node.
        // This sometimes happens on Infura.
        let current_block = self
            .client
            .inner
            .block_number()
            .await?
            .as_usize()
            .saturating_sub(1);

        let last_processed_block = self.base_fee_statistics.last_processed_block();

        if current_block > last_processed_block {
            let n_blocks = current_block - last_processed_block;
            let fee_data = self
                .client
                .inner
                .base_fee_history(current_block, n_blocks)
                .await?;

            // We shouldn't rely on L1 provider to return consistent results, so we check that we have at least one new sample.
            if let Some(current_base_fee_per_gas) = fee_data.last().map(|fee| fee.base_fee_per_gas)
            {
                METRICS
                    .current_base_fee_per_gas
                    .set(current_base_fee_per_gas);
            }
            self.base_fee_statistics
                .add_samples(fee_data.iter().map(|fee| fee.base_fee_per_gas));

            if let Some(current_blob_base_fee) =
                fee_data.last().map(|fee| fee.base_fee_per_blob_gas)
            {
                // Blob base fee overflows `u64` only in very extreme cases.
                // It isn't worth to observe exact value with metric because anyway values that can be used
                // are capped by `self.config.max_blob_base_fee()` of `u64` type.
                if current_blob_base_fee > U256::from(u64::MAX) {
                    tracing::error!("Failed to report current_blob_base_fee = {current_blob_base_fee}, it exceeds u64::MAX");
                } else {
                    METRICS
                        .current_blob_base_fee
                        .set(current_blob_base_fee.as_u64());
                }
            }
            self.blob_base_fee_statistics
                .add_samples(fee_data.iter().map(|fee| fee.base_fee_per_blob_gas));

            if let Some(current_l2_pubdata_price) = fee_data.last().map(|fee| fee.l2_pubdata_price)
            {
                // L2 pubdata price overflows `u64` only in very extreme cases.
                // It isn't worth to observe exact value with metric because anyway values that can be used
                // are capped by `self.config.max_blob_base_fee()` of `u64` type.
                if current_l2_pubdata_price > U256::from(u64::MAX) {
                    tracing::error!("Failed to report current_l2_pubdata_price = {current_l2_pubdata_price}, it exceeds u64::MAX");
                } else {
                    METRICS
                        .current_l2_pubdata_price
                        .set(current_l2_pubdata_price.as_u64());
                }
            }
            self.l2_pubdata_price_statistics
                .add_samples(fee_data.iter().map(|fee| fee.l2_pubdata_price));

            self.gas_per_pubdata_price_statistic
                .add_samples(fee_data.iter().map(|base_fee| base_fee.gas_per_pubdata()));
        }
        Ok(())
    }

    fn bound_gas_price(&self, gas_price: u64) -> u64 {
        let max_l1_gas_price = self.config.max_l1_gas_price;
        if gas_price > max_l1_gas_price {
            tracing::warn!(
                "Effective gas price is too high: {gas_price}, using max allowed: {}",
                max_l1_gas_price
            );
            return max_l1_gas_price;
        }
        gas_price
    }

    pub async fn run(self: Arc<Self>, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, gas_adjuster is shutting down");
                break;
            }

            if let Err(err) = self.keep_updated().await {
                tracing::warn!("Cannot add the base fee to gas statistics: {}", err);
            }

            tokio::time::sleep(self.config.poll_period).await;
        }
        Ok(())
    }

    /// Returns the sum of base and priority fee, in wei, not considering time in mempool.
    /// Can be used to get an estimate of current gas price.
    pub(crate) fn estimate_effective_gas_price(&self) -> u64 {
        if let Some(price) = self.config.internal_enforced_l1_gas_price {
            return price;
        }

        let effective_gas_price = self.get_base_fee(0) + self.get_priority_fee();

        let calculated_price =
            (self.config.internal_l1_pricing_multiplier * effective_gas_price as f64) as u64;

        // Bound the price if it's too high.
        self.bound_gas_price(calculated_price)
    }

    pub(crate) fn estimate_effective_pubdata_price(&self) -> u64 {
        if let Some(price) = self.config.internal_enforced_pubdata_price {
            return price;
        }

        match self.pubdata_sending_mode {
            PubdataSendingMode::Blobs => {
                const BLOB_GAS_PER_BYTE: u64 = 1; // `BYTES_PER_BLOB` = `GAS_PER_BLOB` = 2 ^ 17.

                let blob_base_fee_median = self.blob_base_fee_statistics.median();

                // Check if blob base fee overflows `u64` before converting. Can happen only in very extreme cases.
                if blob_base_fee_median > U256::from(u64::MAX) {
                    let max_allowed = self.config.max_blob_base_fee;
                    tracing::error!("Blob base fee is too high: {blob_base_fee_median}, using max allowed: {max_allowed}");
                    return max_allowed;
                }
                METRICS
                    .median_blob_base_fee
                    .set(blob_base_fee_median.as_u64());
                let calculated_price = blob_base_fee_median.as_u64() as f64
                    * BLOB_GAS_PER_BYTE as f64
                    * self.config.internal_pubdata_pricing_multiplier;

                self.cap_pubdata_fee(calculated_price)
            }
            PubdataSendingMode::Calldata => self.cap_pubdata_fee(
                (self.estimate_effective_gas_price() * L1_GAS_PER_PUBDATA_BYTE as u64) as f64,
            ),
            PubdataSendingMode::Custom => {
                // Fix this when we have a better understanding of dynamic pricing for custom DA layers.
                // GitHub issue: https://github.com/matter-labs/zksync-era/issues/2105
                0
            }
            PubdataSendingMode::RelayedL2Calldata => {
                self.cap_pubdata_fee(self.l2_pubdata_price_statistics.median().as_u64() as f64)
            }
        }
    }

    fn cap_pubdata_fee(&self, pubdata_fee: f64) -> u64 {
        // We will treat the max blob base fee as the maximal fee that we can take for each byte of pubdata.
        let max_blob_base_fee = self.config.max_blob_base_fee;
        match self.commitment_mode {
            L1BatchCommitmentMode::Validium => 0,
            L1BatchCommitmentMode::Rollup => {
                if pubdata_fee > max_blob_base_fee as f64 {
                    tracing::error!("Blob base fee is too high: {pubdata_fee}, using max allowed: {max_blob_base_fee}");
                    return max_blob_base_fee;
                }
                pubdata_fee as u64
            }
        }
    }

    fn calculate_price_with_formula(&self, time_in_mempool_in_l1_blocks: u32, value: u64) -> u64 {
        let a = self.config.pricing_formula_parameter_a;
        let b = self.config.pricing_formula_parameter_b;

        // Currently we use an exponential formula.
        // The alternative is a linear one:
        // `let scale_factor = a + b * time_in_mempool_in_l1_blocks as f64;`
        let scale_factor = a * b.powf(time_in_mempool_in_l1_blocks as f64);
        let new_fee = value as f64 * scale_factor;
        new_fee as u64
    }
}

impl TxParamsProvider for GasAdjuster {
    // This is the method where we decide how much we are ready to pay for the
    // base_fee based on the number of L1 blocks the transaction has been in the mempool.
    // This is done in order to avoid base_fee spikes (e.g. during NFT drops) and
    // smooth out base_fee increases in general.
    // In other words, in order to pay less fees, we are ready to wait longer.
    // But the longer we wait, the more we are ready to pay.
    fn get_base_fee(&self, time_in_mempool_in_l1_blocks: u32) -> u64 {
        let median = self.base_fee_statistics.median();
        METRICS.median_base_fee_per_gas.set(median);
        self.calculate_price_with_formula(time_in_mempool_in_l1_blocks, median)
    }

    fn get_next_block_minimal_base_fee(&self) -> u64 {
        let last_block_base_fee = self.base_fee_statistics.last_added_value();

        // The next block's base fee will decrease by a maximum of 12.5%.
        last_block_base_fee * 875 / 1000
    }

    // Priority fee is set to constant, sourced from config.
    // Reasoning behind this is the following:
    // High `priority_fee` means high demand for block space,
    // which means `base_fee` will increase, which means `priority_fee`
    // will decrease. The EIP-1559 mechanism is designed such that
    // `base_fee` will balance out `priority_fee` in such a way that
    // `priority_fee` will be a small fraction of the overall fee.
    fn get_priority_fee(&self) -> u64 {
        self.config.default_priority_fee_per_gas
    }

    // The idea is that when we finally decide to send blob tx, we want to offer gas fees high
    // enough to "almost be certain" that the transaction gets included. To never have to double
    // the gas prices as then we have very little control how much we pay in the end. This strategy
    // works as no matter if we double or triple such price, we pay the same block base fees.
    fn get_blob_tx_base_fee(&self) -> u64 {
        self.base_fee_statistics.last_added_value() * 2
    }

    fn get_blob_tx_blob_base_fee(&self) -> u64 {
        self.blob_base_fee_statistics.last_added_value().as_u64() * 2
    }

    fn get_blob_tx_priority_fee(&self) -> u64 {
        self.get_priority_fee() * 2
    }

    fn get_gateway_l2_pubdata_price(&self, time_in_mempool_in_l1_blocks: u32) -> u64 {
        let median = self.l2_pubdata_price_statistics.median().as_u64();
        METRICS.median_l2_pubdata_price.set(median);
        self.calculate_price_with_formula(time_in_mempool_in_l1_blocks, median)
    }

    fn get_gateway_price_per_pubdata(&self, time_in_mempool_in_l1_blocks: u32) -> u64 {
        let median = self.gas_per_pubdata_price_statistic.median();
        METRICS.median_gas_per_pubdata_price.set(median);
        self.calculate_price_with_formula(time_in_mempool_in_l1_blocks, median)
    }
}

/// Helper structure responsible for collecting the data about recent transactions,
/// calculating the median base fee.
#[derive(Debug, Clone, Default)]
pub(super) struct GasStatisticsInner<T> {
    samples: VecDeque<T>,
    median_cached: T,
    max_samples: usize,
    last_processed_block: usize,
}

impl<T: Ord + Copy + Default> GasStatisticsInner<T> {
    fn new(max_samples: usize, block: usize, fee_history: impl IntoIterator<Item = T>) -> Self {
        let mut statistics = Self {
            max_samples,
            samples: VecDeque::with_capacity(max_samples),
            median_cached: T::default(),
            last_processed_block: 0,
        };

        statistics.add_samples(fee_history);

        Self {
            last_processed_block: block,
            ..statistics
        }
    }

    fn median(&self) -> T {
        self.median_cached
    }

    fn last_added_value(&self) -> T {
        self.samples.back().copied().unwrap_or(self.median_cached)
    }

    fn add_samples(&mut self, fees: impl IntoIterator<Item = T>) {
        let old_len = self.samples.len();
        self.samples.extend(fees);
        let processed_blocks = self.samples.len() - old_len;
        self.last_processed_block += processed_blocks;

        let extra = self.samples.len().saturating_sub(self.max_samples);
        self.samples.drain(..extra);

        let mut samples: Vec<_> = self.samples.iter().cloned().collect();

        if !self.samples.is_empty() {
            let (_, &mut median, _) = samples.select_nth_unstable(self.samples.len() / 2);
            self.median_cached = median;
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct GasStatistics<T>(RwLock<GasStatisticsInner<T>>);

impl<T: Ord + Copy + Default> GasStatistics<T> {
    pub fn new(max_samples: usize, block: usize, fee_history: impl IntoIterator<Item = T>) -> Self {
        Self(RwLock::new(GasStatisticsInner::new(
            max_samples,
            block,
            fee_history,
        )))
    }

    pub fn median(&self) -> T {
        self.0.read().unwrap().median()
    }

    pub fn last_added_value(&self) -> T {
        self.0.read().unwrap().last_added_value()
    }

    pub fn add_samples(&self, fees: impl IntoIterator<Item = T>) {
        self.0.write().unwrap().add_samples(fees)
    }

    pub fn last_processed_block(&self) -> usize {
        self.0.read().unwrap().last_processed_block
    }
}
