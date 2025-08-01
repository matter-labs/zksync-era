use std::time::Duration;

use anyhow::bail;
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementLayer, L2ChainId};
use zksync_config::configs::contracts::SettlementLayerSpecificContracts;
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::{ContractCallError, EthInterface};
use zksync_settlement_layer_data::{current_settlement_layer, SettlementLayerError};

pub mod node;

#[derive(Debug, thiserror::Error)]
pub enum GatewayMigratorError {
    #[error("ContractCall Error: {0}")]
    ContractCall(#[from] ContractCallError),
    #[error("Error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Gateway Migrator component
/// Component checks the current settlement layer and once it changed and it safe to exit
/// it raised an error forcing server to restart
#[derive(Debug)]
pub struct GatewayMigrator {
    eth_client: Box<dyn EthInterface>,
    gateway_client: Option<Box<dyn EthInterface>>,
    l1_settlement_layer_specific_contracts: SettlementLayerSpecificContracts,
    settlement_layer: Option<SettlementLayer>,
    l2_chain_id: L2ChainId,
    getters_facet_abi: Contract,
    eth_node_poll_interval: Duration,
}

impl GatewayMigrator {
    pub fn new(
        eth_client: Box<dyn EthInterface>,
        gateway_client: Option<Box<dyn EthInterface>>,
        l2_chain_id: L2ChainId,
        settlement_layer: Option<SettlementLayer>,
        l1_settlement_layer_specific_contracts: SettlementLayerSpecificContracts,
        eth_node_poll_interval: Duration,
    ) -> Self {
        let abi = getters_facet_contract();
        Self {
            eth_client,
            gateway_client,
            l1_settlement_layer_specific_contracts,
            settlement_layer,
            l2_chain_id,
            getters_facet_abi: abi,
            eth_node_poll_interval,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let gateway_client = self.gateway_client.as_deref();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, GatewayMigrator is shutting down");
                return Ok(());
            }
            let current_settlement_layer = current_settlement_layer(
                self.eth_client.as_ref(),
                gateway_client,
                &self.l1_settlement_layer_specific_contracts,
                self.l2_chain_id,
                &self.getters_facet_abi,
            )
            .await;

            match current_settlement_layer {
                Ok(current_settlement_layer) => {
                    if self.settlement_layer
                        != current_settlement_layer.settlement_layer_for_sending_txs()
                    {
                        bail!(
                            "Settlement layer changed, from {:?} to {:?}",
                            self.settlement_layer,
                            current_settlement_layer.settlement_layer_for_sending_txs()
                        );
                    }
                }
                Err(SettlementLayerError::ContractCall(ContractCallError::EthereumGateway(
                    err,
                ))) if err.is_retryable() => {
                    tracing::info!("Transient error fetching data from SL: {err}");
                }
                Err(err) => {
                    // If we have an error related to the getting data from the contract,
                    // it's safe to ignore it and continue the loop. The only real problem
                    // could be with missconfigured contracts, but in this case,
                    // other components will fail
                    tracing::error!("Failed to fetch data from SL: {err}");
                }
            }

            tokio::time::sleep(self.eth_node_poll_interval).await;
        }
    }
}
