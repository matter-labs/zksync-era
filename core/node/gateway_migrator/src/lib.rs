use std::time::Duration;

use anyhow::{bail, Context};
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementLayer, L2ChainId, SLChainId};
use zksync_config::configs::contracts::SettlementLayerSpecificContracts;
use zksync_contracts::bridgehub_contract;
use zksync_eth_client::{
    contracts_loader::get_settlement_layer_from_bridgehub, ContractCallError, EthInterface,
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
    settlement_layer: SettlementLayer,
    l2_chain_id: L2ChainId,
    bridgehub_abi: Contract,
}

impl GatewayMigrator {
    pub fn new(
        eth_client: Box<dyn EthInterface>,
        gateway_client: Option<Box<dyn EthInterface>>,
        initial_settlement_layer: SettlementLayer,
        l2_chain_id: L2ChainId,
        l1_settlement_layer_specific_contracts: SettlementLayerSpecificContracts,
    ) -> Self {
        let abi = bridgehub_contract();
        Self {
            eth_client,
            gateway_client,
            l1_settlement_layer_specific_contracts,
            settlement_layer: initial_settlement_layer,
            l2_chain_id,
            bridgehub_abi: abi,
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
                &self.bridgehub_abi,
            )
            .await;

            match current_settlement_layer {
                Ok(current_settlement_layer) => {
                    if self.settlement_layer != current_settlement_layer {
                        bail!("Settlement layer changed")
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

// Return current settlement layer.
pub async fn current_settlement_layer(
    l1_client: &dyn EthInterface,
    gateway_client: Option<&dyn EthInterface>,
    sl_l1_contracts: &SettlementLayerSpecificContracts,
    l2_chain_id: L2ChainId,
    bridgehub_abi: &Contract,
) -> Result<SettlementLayer, GatewayMigratorError> {
    let l1_chain_id = l1_client
        .fetch_chain_id()
        .await
        .map_err(|e| GatewayMigratorError::Internal(e.into()))?;

    let l1_bridgehub = sl_l1_contracts
        .ecosystem_contracts
        .bridgehub_proxy_addr
        .unwrap();

    let settlement_mode_from_l1 = get_settlement_layer_from_bridgehub(
        l1_client,
        l1_chain_id,
        l1_bridgehub,
        SLChainId(l2_chain_id.as_u64()),
        bridgehub_abi,
    )
    .await?
    .context("Chain does not have settlement layer on L1")?;

    let final_settlement_mode = match settlement_mode_from_l1 {
        sl_layer @ SettlementLayer::L1(_) => {
            // Bridgehub on L1 is only toggled when chain is indeed ready to settle on L1.
            sl_layer
        }
        SettlementLayer::Gateway(gw_chain_id) => {
            let gw_client = gateway_client.context("Missing gateway client")?;
            let gw_chain_id_from_provider = gw_client
                .fetch_chain_id()
                .await
                .map_err(|e| GatewayMigratorError::Internal(e.into()))?;
            assert_eq!(
                gw_chain_id, gw_chain_id_from_provider,
                "GW chain id is not the same as in the GW provider"
            );

            // We only need to check whether GW is ready

            let settlement_layer_from_gw = get_settlement_layer_from_bridgehub(
                gw_client,
                l1_chain_id,
                L2_BRIDGEHUB_ADDRESS,
                SLChainId(l2_chain_id.as_u64()),
                bridgehub_abi,
            )
            .await?;

            match settlement_layer_from_gw {
                None => {
                    // This means that the chain has never settled on top of GW.
                    // We should stick to using L1 until the migration finishes.
                    SettlementLayer::L1(l1_chain_id)
                }
                Some(SettlementLayer::Gateway(chain_id_from_gw_bridgehub)) => {
                    // Inequality is only possible if there are more than 1 gateways, but the server is
                    // not ready for this case.
                    assert_eq!(
                        gw_chain_id_from_provider, chain_id_from_gw_bridgehub,
                        "Unexpected GW chain id"
                    );

                    // The migration to Gateway has finished, we need to use GW.
                    SettlementLayer::Gateway(gw_chain_id_from_provider)
                }
                Some(SettlementLayer::L1(_)) => {
                    // Gateway has started migration to L1, but the L1 is not yet ready.
                    // We have to use Gateway
                    SettlementLayer::Gateway(gw_chain_id_from_provider)
                }
            }
        }
    };

    Ok(final_settlement_mode)
}
