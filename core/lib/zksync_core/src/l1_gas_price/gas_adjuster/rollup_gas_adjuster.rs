use std::sync::Arc;

use async_trait::async_trait;
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::{Error, EthInterface};
use zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE;

use super::{GasAdjuster, GasStatistics, L1TxParamsProvider};

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
