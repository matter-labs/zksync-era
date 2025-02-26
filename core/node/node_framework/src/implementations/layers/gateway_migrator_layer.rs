use async_trait::async_trait;
use tokio::{sync::oneshot, task::JoinHandle};
use zksync_contracts::getters_facet_contract;
use zksync_eth_client::clients::PKSigningClient;
use zksync_multilayer_client::{get_settlement_layer, GatewayMigrator};
use zksync_types::Address;

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource, settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext, StopReceiver, Task, TaskId,
};

/// Wiring layer for [`PKSigningClient`].
#[derive(Debug)]
pub struct GatewayMigratorLayer {
    diamond_proxy_addr: Address,
}

impl GatewayMigratorLayer {
    pub fn new(diamond_proxy_addr: Address) -> Self {
        Self { diamond_proxy_addr }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub eth_client: EthInterfaceResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    initial_settlement_mode: SettlementModeResource,
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
        let migrator = GatewayMigrator::new(input.eth_client.0, self.diamond_proxy_addr).await;

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(migrator.settlement_mode()),
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
