use async_trait::async_trait;
use zksync_gateway_migrator::GatewayMigrator;

use crate::{
    implementations::resources::{
        contracts::SettlementLayerContractsResource, eth_interface::EthInterfaceResource,
        settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext, StopReceiver, Task, TaskId,
};

/// Wiring layer for [`GatewayMigrator`].
#[derive(Debug)]
pub struct GatewayMigratorLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    eth_client: EthInterfaceResource,
    contracts: SettlementLayerContractsResource,
    settlement_mode_resource: SettlementModeResource,
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
            input.eth_client.0,
            input.contracts.0.chain_contracts_config.diamond_proxy_addr,
            input.settlement_mode_resource.0,
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
        self.run_inner(stop_receiver.0).await
    }
}
