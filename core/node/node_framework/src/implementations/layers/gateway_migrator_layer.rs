use async_trait::async_trait;
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
    pub l2chain_id: L2ChainId,
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
            input.contracts.0.chain_contracts_config.diamond_proxy_addr,
            input.settlement_mode_resource.0,
            self.l2chain_id,
            input
                .contracts
                .0
                .ecosystem_contracts
                .bridgehub_proxy_addr
                .expect("bridgehub_proxy_addr"),
            input.pool.get().await?,
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
