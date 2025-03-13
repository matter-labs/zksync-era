use std::{fmt::Debug, sync::Arc, time::Duration};

use anyhow::bail;
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementMode, Address, L2ChainId};
use zksync_contracts::getters_facet_contract;
use zksync_contracts_loader::{
    get_settlement_layer_address, get_settlement_layer_from_l1, load_settlement_layer_contracts,
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
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
    settlement_mode: SettlementMode,
    l2chain_id: L2ChainId,
    abi: Contract,
    pool: ConnectionPool<Core>,
}

impl GatewayMigrator {
    pub fn new(
        eth_client: Box<dyn EthInterface>,
        gateway_client: Option<Box<dyn EthInterface>>,
        l1_diamond_proxy_addr: Address,
        initial_settlement_mode: SettlementMode,
        l2chain_id: L2ChainId,
        pool: ConnectionPool<Core>,
    ) -> Self {
        let abi = getters_facet_contract();
        Self {
            eth_client,
            gateway_client,
            l1_diamond_proxy_addr,
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
            let (settlement_mode, _) = get_settlement_layer_from_l1(
                self.eth_client.as_ref(),
                self.l1_diamond_proxy_addr,
                &self.abi,
            )
            .await?;

            if settlement_mode != self.settlement_mode
                && switch_to_current_settlement_mode(
                    settlement_mode,
                    gateway_client.clone().as_deref(),
                    self.l2chain_id,
                    &mut self.pool.connection().await?,
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
    settlement_mode_from_l1: SettlementMode,
    gateway_client: Option<&dyn EthInterface>,
    l2chain_id: L2ChainId,
    storage: &mut Connection<'_, Core>,
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

    let res = match settlement_mode_from_l1 {
        // We got the settlement mode from l1 initially, it's safe to switch to this settlement mode
        SettlementMode::SettlesToL1 => true,
        SettlementMode::Gateway => {
            // Load chain contracts from gateway
            let gateway_client = gateway_client.unwrap();

            let sl_contracts = load_settlement_layer_contracts(
                gateway_client,
                L2_BRIDGEHUB_ADDRESS,
                l2chain_id,
                None,
            )
            .await?;
            // Wait until the contracts are deployed on l2
            if let Some(contracts) = sl_contracts {
                let settlement_layer_address = get_settlement_layer_address(
                    gateway_client,
                    contracts.chain_contracts_config.diamond_proxy_addr,
                    abi,
                )
                .await?;
                // When we settle to the current chain, settlement mode should zero
                settlement_layer_address.is_zero()
            } else {
                false
            }
        }
    };
    Ok(res)
}
