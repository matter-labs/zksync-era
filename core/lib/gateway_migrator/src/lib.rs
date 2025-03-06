use std::{fmt::Debug, time::Duration};

use anyhow::bail;
use tokio::sync::watch;
use zksync_basic_types::{ethabi::Contract, settlement::SettlementMode, Address};
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::{CallFunctionArgs, EthInterface};

#[derive(Debug)]
pub struct GatewayMigrator {
    sl_client: Box<dyn EthInterface>,
    diamond_proxy_addr: Address,
    settlement_mode: SettlementMode,
    abi: Contract,
}

impl GatewayMigrator {
    pub fn new(
        sl_client: Box<dyn EthInterface>,
        diamond_proxy_addr: Address,
        initial_settlement_mode: SettlementMode,
    ) -> Self {
        let abi = getters_facet_contract();
        Self {
            sl_client,
            diamond_proxy_addr,
            settlement_mode: initial_settlement_mode,
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
            let settlement_mode =
                get_settlement_layer(self.sl_client.as_ref(), self.diamond_proxy_addr, &self.abi)
                    .await
                    .unwrap();

            if settlement_mode != self.settlement_mode {
                bail!("Settlement layer changed")
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

pub async fn get_settlement_layer(
    eth_client: &dyn EthInterface,
    diamond_proxy_addr: Address,
    abi: &Contract,
) -> anyhow::Result<SettlementMode> {
    let settlement_layer: Address = CallFunctionArgs::new("getSettlementLayer", ())
        .for_contract(diamond_proxy_addr, abi)
        .call(eth_client)
        .await?;

    let mode = if settlement_layer.is_zero() {
        SettlementMode::SettlesToL1
    } else {
        SettlementMode::Gateway
    };

    Ok(mode)
}
