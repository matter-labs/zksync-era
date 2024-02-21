use std::sync::Arc;

use async_trait::async_trait;
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::{Error, EthInterface};
use zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE;

use super::{metrics::METRICS, GasAdjuster, GasStatistics, L1TxParamsProvider};

#[derive(Debug)]
pub struct RollupGasAdjuster {
    pub(super) statistics: GasStatistics,
    pub(super) config: GasAdjusterConfig,
    eth_client: Arc<dyn EthInterface>,
}

#[async_trait]
impl GasAdjuster for RollupGasAdjuster {
    fn config(&self) -> &GasAdjusterConfig {
        &self.config
    }

    fn eth_client(&self) -> &dyn EthInterface {
        &self.eth_client
    }

    fn statistics(&self) -> &GasStatistics {
        &self.statistics
    }

    fn into_l1_tx_params_provider(self: Arc<Self>) -> Arc<dyn L1TxParamsProvider> {
        self
    }

    fn estimate_effective_pubdata_price(&self) -> u64 {
        // For now, pubdata is only sent via calldata, so its price is pegged to the L1 gas price.
        self.estimate_effective_gas_price() * L1_GAS_PER_PUBDATA_BYTE as u64
    }
}

impl RollupGasAdjuster {
    pub async fn new(
        eth_client: Arc<dyn EthInterface>,
        config: GasAdjusterConfig,
    ) -> Result<Self, Error> {
        // Subtracting 1 from the "latest" block number to prevent errors in case
        // the info about the latest block is not yet present on the node.
        // This sometimes happens on Infura.
        let current_block = eth_client
            .block_number("gas_adjuster")
            .await?
            .as_usize()
            .saturating_sub(1);
        let history = eth_client
            .base_fee_history(current_block, config.max_base_fee_samples, "gas_adjuster")
            .await?;
        Ok(Self {
            statistics: GasStatistics::new(config.max_base_fee_samples, current_block, &history),
            eth_client,
            config,
        })
    }
}

impl L1TxParamsProvider for RollupGasAdjuster {
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
        let median = self.statistics.median();
        METRICS.median_base_fee_per_gas.set(median);
        let new_fee = median as f64 * scale_factor;
        new_fee as u64
    }

    fn get_next_block_minimal_base_fee(&self) -> u64 {
        let last_block_base_fee = self.statistics.last_added_value();

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
}
