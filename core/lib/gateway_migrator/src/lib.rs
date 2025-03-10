use std::{fmt::Debug, time::Duration};

use anyhow::bail;
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementMode, Address, L2ChainId};
use zksync_contracts::getters_facet_contract;
use zksync_contracts_loader::{get_settlement_layer, load_sl_contracts};
use zksync_eth_client::EthInterface;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

#[derive(Debug)]
pub struct GatewayMigrator {
    eth_client: Box<dyn EthInterface>,
    gateway_client: Option<Box<dyn EthInterface>>,
    l1_diamond_proxy_addr: Address,
    settlement_mode: SettlementMode,
    l2chain_id: L2ChainId,
    abi: Contract,
}

impl GatewayMigrator {
    pub fn new(
        eth_client: Box<dyn EthInterface>,
        gateway_client: Option<Box<dyn EthInterface>>,
        l1_diamond_proxy_addr: Address,
        initial_settlement_mode: SettlementMode,
        l2chain_id: L2ChainId,
    ) -> Self {
        let abi = getters_facet_contract();
        Self {
            eth_client,
            gateway_client,
            l1_diamond_proxy_addr,
            settlement_mode: initial_settlement_mode,
            l2chain_id,
            abi,
        }
    }

    pub fn settlement_mode(&self) -> SettlementMode {
        self.settlement_mode
    }
    pub async fn run_inner(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        // let mut attempts = 0;
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, GatewayMigrator is shutting down");
                return Ok(());
            }
            let settlement_mode = get_settlement_layer(
                self.eth_client.as_ref(),
                self.l1_diamond_proxy_addr,
                &self.abi,
            )
            .await?;

            if settlement_mode != self.settlement_mode {
                match settlement_mode {
                    SettlementMode::SettlesToL1 => {
                        bail!("Settlement layer changed")
                    }
                    SettlementMode::Gateway => {
                        let sl_contracts = load_sl_contracts(
                            self.gateway_client.as_ref().unwrap().as_ref(),
                            L2_BRIDGEHUB_ADDRESS,
                            self.l2chain_id,
                            None,
                        )
                        .await?;
                        // Wait until the contracts are deployed on l2
                        if sl_contracts.is_some() {
                            bail!("Settlement layer changed")
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
