use std::time::Duration;

use anyhow::bail;
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementLayer, L2ChainId};
use zksync_config::configs::contracts::SettlementLayerSpecificContracts;
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::{
    contracts_loader::{
        get_diamond_proxy_contract, get_settlement_layer_address, get_settlement_layer_from_l1,
    },
    ContractCallError, EthInterface,
};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

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
}

impl GatewayMigrator {
    pub fn new(
        eth_client: Box<dyn EthInterface>,
        gateway_client: Option<Box<dyn EthInterface>>,
        l2_chain_id: L2ChainId,
        settlement_layer: Option<SettlementLayer>,
        l1_settlement_layer_specific_contracts: SettlementLayerSpecificContracts,
    ) -> Self {
        let abi = getters_facet_contract();
        Self {
            eth_client,
            gateway_client,
            l1_settlement_layer_specific_contracts,
            settlement_layer,
            l2_chain_id,
            getters_facet_abi: abi,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let gateway_client = self.gateway_client.as_deref();
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, GatewayMigrator is shutting down");
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
                Err(GatewayMigratorError::ContractCall(ContractCallError::EthereumGateway(
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

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

/// During the migration settlement layer could be unknown for the server.
/// In this case, we should not use it for sending transactions.
/// Meanwhile we should continue to work with the old settlement layer.
#[derive(Debug, Clone)]
pub struct WorkingSettlementLayer {
    unsafe_settlement_layer: SettlementLayer,
    migration_in_progress: bool,
}

impl WorkingSettlementLayer {
    pub fn new(unsafe_settlement_layer: SettlementLayer) -> Self {
        Self {
            unsafe_settlement_layer,
            migration_in_progress: false,
        }
    }
}

impl WorkingSettlementLayer {
    pub fn settlement_layer(&self) -> SettlementLayer {
        self.unsafe_settlement_layer
    }

    pub fn settlement_layer_for_sending_txs(&self) -> Option<SettlementLayer> {
        if self.migration_in_progress {
            None
        } else {
            Some(self.unsafe_settlement_layer)
        }
    }
}

// Return current settlement layer.
pub async fn current_settlement_layer(
    l1_client: &dyn EthInterface,
    gateway_client: Option<&dyn EthInterface>,
    sl_l1_contracts: &SettlementLayerSpecificContracts,
    l2_chain_id: L2ChainId,
    abi: &Contract,
) -> Result<WorkingSettlementLayer, GatewayMigratorError> {
    let settlement_mode_from_l1 = get_settlement_layer_from_l1(
        l1_client,
        sl_l1_contracts.chain_contracts_config.diamond_proxy_addr,
        abi,
    )
    .await?;

    let (sl_client, bridge_hub_address) = match settlement_mode_from_l1 {
        SettlementLayer::L1(_) => (
            l1_client,
            sl_l1_contracts
                .ecosystem_contracts
                .bridgehub_proxy_addr
                .unwrap(),
        ),
        SettlementLayer::Gateway(_) => (gateway_client.unwrap(), L2_BRIDGEHUB_ADDRESS),
    };

    // Load chain contracts from sl
    let diamond_proxy_addr =
        get_diamond_proxy_contract(sl_client, bridge_hub_address, l2_chain_id).await?;
    // Deploying contracts on gateway are going through l1->l2 communication,
    // even though the settlement layer has changed on l1.
    // Gateway should process l1->l2 transaction.
    // Even though when we switched from gateway to l1,
    // we don't need to wait for contracts deployment,
    // we have to wait for l2->l1 communication to be finalized
    let use_settlement_mode_from_l1 = if !diamond_proxy_addr.is_zero() {
        let settlement_layer_address =
            get_settlement_layer_address(sl_client, diamond_proxy_addr, abi).await?;

        // When we settle to the current chain, settlement mode should zero
        settlement_layer_address.is_zero()
    } else {
        match settlement_mode_from_l1 {
            // if we want to settle to l1, but no contracts deployed, that means it's pre gateway upgrade and we need to settle to l1
            SettlementLayer::L1(_) => true,
            // if we want to settle to gateway, but no contracts deployed, that means the migration has not been completed yet. We need to continue settle to L1
            SettlementLayer::Gateway(_) => false,
        }
    };

    let final_settlement_mode = if use_settlement_mode_from_l1 {
        settlement_mode_from_l1
    } else {
        // If it's impossible to use settlement_mode_from_l1 server have to use the opposite settlement_layer
        match settlement_mode_from_l1 {
            SettlementLayer::L1(_) => {
                let chain_id = gateway_client
                    .unwrap()
                    .fetch_chain_id()
                    .await
                    .map_err(ContractCallError::from)?;
                SettlementLayer::Gateway(chain_id)
            }
            SettlementLayer::Gateway(_) => {
                let chain_id = l1_client
                    .fetch_chain_id()
                    .await
                    .map_err(ContractCallError::from)?;
                SettlementLayer::L1(chain_id)
            }
        }
    };

    Ok(WorkingSettlementLayer {
        unsafe_settlement_layer: final_settlement_mode,
        migration_in_progress: !use_settlement_mode_from_l1,
    })
}
