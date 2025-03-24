use std::{fmt::Debug, sync::Arc, time::Duration};

use anyhow::bail;
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementLayer, Address, L2ChainId};
use zksync_contracts::getters_facet_contract;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::contracts_loader::{
    get_settlement_layer_address, get_settlement_layer_from_l1, load_settlement_layer_contracts,
};
use zksync_eth_client::EthInterface;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

/// Gateway Migrator component
/// Component checks the current settlement layer and once it changed and it safe to exit
/// it raised an error forcing server to restart
#[derive(Debug)]
pub struct GatewayMigrator {
    eth_client: Box<dyn EthInterface>,
    gateway_client: Option<Box<dyn EthInterface>>,
    l1_diamond_proxy_addr: Address,
    l1_bridge_hub_address: Address,
    settlement_mode: SettlementLayer,
    l2chain_id: L2ChainId,
    abi: Contract,
    pool: ConnectionPool<Core>,
}

impl GatewayMigrator {
    pub fn new(
        eth_client: Box<dyn EthInterface>,
        gateway_client: Option<Box<dyn EthInterface>>,
        l1_diamond_proxy_addr: Address,
        initial_settlement_mode: SettlementLayer,
        l2chain_id: L2ChainId,
        l1_bridge_hub_address: Address,
        pool: ConnectionPool<Core>,
    ) -> Self {
        let abi = getters_facet_contract();
        Self {
            eth_client,
            gateway_client,
            l1_diamond_proxy_addr,
            l1_bridge_hub_address,
            settlement_mode: initial_settlement_mode,
            l2chain_id,
            abi,
            pool,
        }
    }

    pub async fn run_inner(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let gateway_client: Option<Arc<dyn EthInterface>> = self.gateway_client.map(|a| a.into());
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, GatewayMigrator is shutting down");
                return Ok(());
            }
            let settlement_mode = get_settlement_layer_from_l1(
                self.eth_client.as_ref(),
                self.l1_diamond_proxy_addr,
                &self.abi,
            )
            .await?;
            let sl = gateway_client.clone();
            let (bridgehub_address, client) = match settlement_mode {
                SettlementLayer::L1(_) => (self.l1_bridge_hub_address, self.eth_client.as_ref()),
                SettlementLayer::Gateway(_) => {
                    (L2_BRIDGEHUB_ADDRESS, sl.as_ref().unwrap().as_ref())
                }
            };
            if settlement_mode != self.settlement_mode
                && switch_to_current_settlement_mode(
                    settlement_mode,
                    client,
                    self.l2chain_id,
                    &mut self.pool.connection().await?,
                    bridgehub_address,
                    &self.abi,
                )
                .await?
            {
                bail!("Settlement layer changed")
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

pub async fn switch_to_current_settlement_mode(
    settlement_mode_from_l1: SettlementLayer,
    sl_client: &dyn EthInterface,
    l2chain_id: L2ChainId,
    storage: &mut Connection<'_, Core>,
    bridge_hub_address: Address,
    abi: &Contract,
) -> anyhow::Result<bool> {
    // Check how many transaction from the opposite settlement mode we have.
    // This function supposed to be used during the start of the server or during the switch.
    // And we can't start with new settlement mode while we have inflight transactions
    let inflight_count = storage
        .eth_sender_dal()
        .get_inflight_txs_count_for_gateway_migration(!settlement_mode_from_l1.is_gateway())
        .await?;

    if inflight_count != 0 {
        return Ok(false);
    }

    // Load chain contracts from sl
    let sl_contracts =
        load_settlement_layer_contracts(sl_client, bridge_hub_address, l2chain_id, None).await?;
    // Deploying contracts on gateway are going through l1->l2 communication,
    // even though the settlement layer has changed on l1.
    // Gateway should process l1->l2 transaction.
    // Even though when we switched from gateway to l1,
    // we don't need to wait for contracts deployment,
    // we have to wait for l2->l1 communication to be finalized
    let res = if let Some(contracts) = sl_contracts {
        let settlement_layer_address = get_settlement_layer_address(
            sl_client,
            contracts.chain_contracts_config.diamond_proxy_addr,
            abi,
        )
        .await?;
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
    Ok(res)
}
