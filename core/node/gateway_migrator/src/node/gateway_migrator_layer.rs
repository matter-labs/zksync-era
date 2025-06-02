use async_trait::async_trait;
use zksync_basic_types::L2ChainId;
use zksync_config::configs::GatewayMigratorConfig;
use zksync_eth_client::{node::contracts::L1ChainContractsResource, EthInterface};
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext, StopReceiver, Task, TaskId,
};
use zksync_web3_decl::{
    client::{DynClient, L1},
    node::{GatewayClientResource, SettlementModeResource},
};

use crate::GatewayMigrator;

/// Wiring layer for [`GatewayMigrator`].
#[derive(Debug)]
pub struct GatewayMigratorLayer {
    pub l2_chain_id: L2ChainId,
    pub gateway_migrator_config: GatewayMigratorConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    eth_client: Box<DynClient<L1>>,
    gateway_client: Option<GatewayClientResource>,
    contracts: L1ChainContractsResource,
    settlement_mode_resource: SettlementModeResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    gateway_migrator: GatewayMigrator,
}

#[async_trait::async_trait]
impl WiringLayer for GatewayMigratorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "gateway_migrator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let migrator = GatewayMigrator::new(
            Box::new(input.eth_client),
            input
                .gateway_client
                .map(|a| Box::new(a.0) as Box<dyn EthInterface>),
            self.l2_chain_id,
            input
                .settlement_mode_resource
                .settlement_layer_for_sending_txs(),
            input.contracts.0,
            self.gateway_migrator_config.eth_node_poll_interval,
        );

        Ok(Output {
            gateway_migrator: migrator,
        })
    }
}

#[async_trait]
impl Task for GatewayMigrator {
    fn id(&self) -> TaskId {
        "gateway_migrator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
