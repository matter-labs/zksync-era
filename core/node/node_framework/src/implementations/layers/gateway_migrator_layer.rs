use async_trait::async_trait;
use zksync_config::configs::GatewayMigratorConfig;
use zksync_eth_client::EthInterface;
use zksync_gateway_migrator::GatewayMigrator;
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        contracts::L1ChainContractsResource,
        eth_interface::{EthInterfaceResource, L2InterfaceResource},
        pools::{MasterPool, PoolResource},
        settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext, StopReceiver, Task, TaskId,
};

/// Wiring layer for [`GatewayMigrator`].
#[derive(Debug)]
pub struct GatewayMigratorLayer {
    pub l2_chain_id: L2ChainId,
    pub gateway_migrator_config: GatewayMigratorConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    eth_client: EthInterfaceResource,
    gateway_client: Option<L2InterfaceResource>,
    contracts: L1ChainContractsResource,
    settlement_mode_resource: SettlementModeResource,
    pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
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
            Box::new(input.eth_client.0),
            input
                .gateway_client
                .map(|a| Box::new(a.0) as Box<dyn EthInterface>),
            input.settlement_mode_resource.0,
            self.l2_chain_id,
            input.pool.get().await?,
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
