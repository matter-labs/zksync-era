//! This module determines the fees to pay in txs containing blocks submitted to the L1.

use std::{
    collections::VecDeque,
    fmt::Debug,
    ops::RangeInclusive,
    sync::{Arc, RwLock},
};

use tokio::sync::watch;
use zksync_config::{configs::eth_sender::PubdataSendingMode, GasAdjusterConfig};
use zksync_eth_client::EthInterface;
use zksync_types::{commitment::L1BatchCommitmentMode, L1_GAS_PER_PUBDATA_BYTE, U256, U64};
use zksync_web3_decl::client::{DynClient, L1};

use self::metrics::METRICS;
use super::L1TxParamsProvider;

mod metrics;
#[cfg(test)]
mod tests;

pub trait L1GasAdjuster: Debug + Send + Sync {
    fn estimate_effective_gas_price(&self) -> u64;
    fn estimate_effective_pubdata_price(&self) -> u64;
}

/// This component keeps track of the median `base_fee` from the last `max_base_fee_samples` blocks
/// and of the median `blob_base_fee` from the last `max_blob_base_fee_sample` blocks.
/// It is used to adjust the base_fee of transactions sent to L1.
#[derive(Debug)]
pub struct GasAdjuster {
    pub(super) base_fee_statistics: GasStatistics<u64>,
    // Type for blob base fee is chosen to be `U256`.
    // In practice, it's very unlikely to overflow `u64` (if `blob_base_fee_statistics` = 10 ^ 18, then price for one blob is 2 ^ 17 ETH).
    // But it's still possible and code shouldn't panic if that happens. One more argument is that geth uses big int type for blob prices.
    pub(super) blob_base_fee_statistics: GasStatistics<U256>,
    pub(super) config: GasAdjusterConfig,
    pubdata_sending_mode: PubdataSendingMode,
    eth_client: Box<DynClient<L1>>,
    commitment_mode: L1BatchCommitmentMode,
}

impl L1GasAdjuster for GasAdjuster {
    fn estimate_effective_gas_price(&self) -> u64 {
        if let Some(price) = self.config.internal_enforced_l1_gas_price {
            return price;
        }

        let effective_gas_price = self.get_base_fee(0) + self.get_priority_fee();

        let calculated_price =
            (self.config.internal_l1_pricing_multiplier * effective_gas_price as f64) as u64;

        // Bound the price if it's too high.
        self.bound_gas_price(calculated_price)
    }

    fn estimate_effective_pubdata_price(&self) -> u64 {
        if let Some(price) = self.config.internal_enforced_pubdata_price {
            return price;
        }

        match self.pubdata_sending_mode {
            PubdataSendingMode::Blobs => {
                const BLOB_GAS_PER_BYTE: u64 = 1; // `BYTES_PER_BLOB` = `GAS_PER_BLOB` = 2 ^ 17.

                let blob_base_fee_median = self.blob_base_fee_statistics.median();

                // Check if blob base fee overflows `u64` before converting. Can happen only in very extreme cases.
                if blob_base_fee_median > U256::from(u64::MAX) {
                    let max_allowed = self.config.max_blob_base_fee();
                    tracing::error!("Blob base fee is too high: {blob_base_fee_median}, using max allowed: {max_allowed}");
                    return max_allowed;
                }
                METRICS
                    .median_blob_base_fee
                    .set(blob_base_fee_median.as_u64());
                let calculated_price = blob_base_fee_median.as_u64() as f64
                    * BLOB_GAS_PER_BYTE as f64
                    * self.config.internal_pubdata_pricing_multiplier;

                self.bound_blob_base_fee(calculated_price)
            }
            PubdataSendingMode::Calldata => {
                self.estimate_effective_gas_price() * self.pubdata_byte_gas()
            }
        }
    }
}

impl GasAdjuster {
    pub async fn new(
        eth_client: Box<DynClient<L1>>,
        config: GasAdjusterConfig,
        pubdata_sending_mode: PubdataSendingMode,
        commitment_mode: L1BatchCommitmentMode,
    ) -> anyhow::Result<Self> {
        let eth_client = eth_client.for_component("gas_adjuster");

        // Subtracting 1 from the "latest" block number to prevent errors in case
        // the info about the latest block is not yet present on the node.
        // This sometimes happens on Infura.
        let current_block = eth_client
            .block_number()
            .await?
            .as_usize()
            .saturating_sub(1);
        let base_fee_history = eth_client
            .base_fee_history(current_block, config.max_base_fee_samples)
            .await?;

        // Web3 API doesn't provide a method to fetch blob fees for multiple blocks using single request,
        // so we request blob base fee only for the latest block.
        let (_, last_block_blob_base_fee) =
            Self::get_base_fees_history(eth_client.as_ref(), current_block..=current_block).await?;

        Ok(Self {
            base_fee_statistics: GasStatistics::new(
                config.max_base_fee_samples,
                current_block,
                &base_fee_history,
            ),
            blob_base_fee_statistics: GasStatistics::new(
                config.num_samples_for_blob_base_fee_estimate,
                current_block,
                &last_block_blob_base_fee,
            ),
            config,
            pubdata_sending_mode,
            eth_client,
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
            .eth_client
            .block_number()
            .await?
            .as_usize()
            .saturating_sub(1);

        let last_processed_block = self.base_fee_statistics.last_processed_block();

        if current_block > last_processed_block {
            let (base_fee_history, blob_base_fee_history) = Self::get_base_fees_history(
                self.eth_client.as_ref(),
                (last_processed_block + 1)..=current_block,
            )
            .await?;

            // We shouldn't rely on L1 provider to return consistent results, so we check that we have at least one new sample.
            if let Some(current_base_fee_per_gas) = base_fee_history.last() {
                METRICS
                    .current_base_fee_per_gas
                    .set(*current_base_fee_per_gas);
            }
            self.base_fee_statistics.add_samples(&base_fee_history);

            if let Some(current_blob_base_fee) = blob_base_fee_history.last() {
                // Blob base fee overflows `u64` only in very extreme cases.
                // It doesn't worth to observe exact value with metric because anyway values that can be used
                // are capped by `self.config.max_blob_base_fee()` of `u64` type.
                if current_blob_base_fee > &U256::from(u64::MAX) {
                    tracing::error!("Failed to report current_blob_base_fee = {current_blob_base_fee}, it exceeds u64::MAX");
                } else {
                    METRICS
                        .current_blob_base_fee
                        .set(current_blob_base_fee.as_u64());
                }
            }
            self.blob_base_fee_statistics
                .add_samples(&blob_base_fee_history);
        }
        Ok(())
    }

    fn bound_gas_price(&self, gas_price: u64) -> u64 {
        let max_l1_gas_price = self.config.max_l1_gas_price();
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
                tracing::info!("Stop signal received, gas_adjuster is shutting down");
                break;
            }

            if let Err(err) = self.keep_updated().await {
                tracing::warn!("Cannot add the base fee to gas statistics: {}", err);
            }

            tokio::time::sleep(self.config.poll_period()).await;
        }
        Ok(())
    }

    fn pubdata_byte_gas(&self) -> u64 {
        match self.commitment_mode {
            L1BatchCommitmentMode::Validium => 0,
            L1BatchCommitmentMode::Rollup => L1_GAS_PER_PUBDATA_BYTE.into(),
        }
    }

    fn bound_blob_base_fee(&self, blob_base_fee: f64) -> u64 {
        let max_blob_base_fee = self.config.max_blob_base_fee();
        match self.commitment_mode {
            L1BatchCommitmentMode::Validium => 0,
            L1BatchCommitmentMode::Rollup => {
                if blob_base_fee > max_blob_base_fee as f64 {
                    tracing::error!("Blob base fee is too high: {blob_base_fee}, using max allowed: {max_blob_base_fee}");
                    return max_blob_base_fee;
                }
                blob_base_fee as u64
            }
        }
    }

    /// Returns vector of base fees and blob base fees for given block range.
    /// Note, that data for pre-dencun blocks won't be included in the vector returned.
    async fn get_base_fees_history(
        eth_client: &DynClient<L1>,
        block_range: RangeInclusive<usize>,
    ) -> anyhow::Result<(Vec<u64>, Vec<U256>)> {
        let mut base_fee_history = Vec::new();
        let mut blob_base_fee_history = Vec::new();
        for block_number in block_range {
            let header = eth_client.block(U64::from(block_number).into()).await?;
            if let Some(base_fee_per_gas) =
                header.as_ref().and_then(|header| header.base_fee_per_gas)
            {
                base_fee_history.push(base_fee_per_gas.as_u64())
            }

            if let Some(excess_blob_gas) = header.as_ref().and_then(|header| header.excess_blob_gas)
            {
                blob_base_fee_history.push(Self::blob_base_fee(excess_blob_gas.as_u64()))
            }
        }

        Ok((base_fee_history, blob_base_fee_history))
    }

    /// Calculates `blob_base_fee` given `excess_blob_gas`.
    fn blob_base_fee(excess_blob_gas: u64) -> U256 {
        // Constants and formula are taken from EIP4844 specification.
        const MIN_BLOB_BASE_FEE: u32 = 1;
        const BLOB_BASE_FEE_UPDATE_FRACTION: u32 = 3338477;

        Self::fake_exponential(
            MIN_BLOB_BASE_FEE.into(),
            excess_blob_gas.into(),
            BLOB_BASE_FEE_UPDATE_FRACTION.into(),
        )
    }

    /// approximates `factor * e ** (numerator / denominator)` using Taylor expansion.
    fn fake_exponential(factor: U256, numerator: U256, denominator: U256) -> U256 {
        let mut i = 1_u32;
        let mut output = U256::zero();
        let mut accum = factor * denominator;
        while !accum.is_zero() {
            output += accum;

            accum *= numerator;
            accum /= denominator;
            accum /= U256::from(i);

            i += 1;
        }

        output / denominator
    }
}

impl L1TxParamsProvider for GasAdjuster {
    // This is the method where we decide how much we are ready to pay for the
    // base_fee based on the number of L1 blocks the transaction has been in the mempool.
    // This is done in order to avoid base_fee spikes (e.g. during NFT drops) and
    // smooth out base_fee increases in general.
    // In other words, in order to pay less fees, we are ready to wait longer.
    // But the longer we wait, the more we are ready to pay.
    fn get_base_fee(&self, time_in_mempool: u32) -> u64 {
        let a = self.config.pricing_formula_parameter_a;
        let b = self.config.pricing_formula_parameter_b;

        // Currently we use an exponential formula.
        // The alternative is a linear one:
        // `let scale_factor = a + b * time_in_mempool as f64;`
        let scale_factor = a * b.powf(time_in_mempool as f64);
        let median = self.base_fee_statistics.median();
        METRICS.median_base_fee_per_gas.set(median);
        let new_fee = median as f64 * scale_factor;
        new_fee as u64
    }

    fn get_blob_base_fee(&self) -> u64 {
        let a = self.config.pricing_formula_parameter_a;
        let b = self.config.pricing_formula_parameter_b;

        // Use the single evaluation at zero of the following:
        // Currently we use an exponential formula.
        // The alternative is a linear one:
        // `let scale_factor = a + b * time_in_mempool as f64;`
        let scale_factor = a * b.powf(0.0);
        let median = self.blob_base_fee_statistics.median();
        METRICS.median_blob_base_fee_per_gas.set(median.as_u64());
        let new_fee = median.as_u64() as f64 * scale_factor;
        new_fee as u64
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
    fn new(max_samples: usize, block: usize, fee_history: &[T]) -> Self {
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

    fn add_samples(&mut self, fees: &[T]) {
        self.samples.extend(fees);
        self.last_processed_block += fees.len();

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
    pub fn new(max_samples: usize, block: usize, fee_history: &[T]) -> Self {
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

    pub fn add_samples(&self, fees: &[T]) {
        self.0.write().unwrap().add_samples(fees)
    }

    pub fn last_processed_block(&self) -> usize {
        self.0.read().unwrap().last_processed_block
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MockGasAdjuster {
    pub effective_l1_gas_price: u64,
    pub effective_l1_pubdata_price: u64,
}

impl L1GasAdjuster for MockGasAdjuster {
    fn estimate_effective_gas_price(&self) -> u64 {
        self.effective_l1_gas_price
    }

    fn estimate_effective_pubdata_price(&self) -> u64 {
        self.effective_l1_pubdata_price
    }
}

impl MockGasAdjuster {
    pub fn new(effective_l1_gas_price: u64, effective_l1_pubdata_price: u64) -> Self {
        Self {
            effective_l1_gas_price,
            effective_l1_pubdata_price,
        }
    }
}
