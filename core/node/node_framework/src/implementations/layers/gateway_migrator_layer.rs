use async_trait::async_trait;
use zksync_config::Contracts;
use zksync_multilayer_client::GatewayMigrator;

use crate::{
    implementations::resources::{
        contracts::ContractsResource, eth_interface::EthInterfaceResource,
        settlement_layer::SettlementModeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext, StopReceiver, Task, TaskId,
};

/// Wiring layer for [`PKSigningClient`].
#[derive(Debug)]
pub struct GatewayMigratorLayer {
    contracts: Contracts,
    stop: bool,
}

impl GatewayMigratorLayer {
    pub fn new(contracts_config: Contracts, stop: bool) -> Self {
        Self {
            contracts: contracts_config,
            stop,
        }
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
    contracts: ContractsResource,
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
            self.contracts
                .current_contracts()
                .chain_contracts_config
                .diamond_proxy_addr,
            self.stop,
        )
        .await;
        let mut contracts = self.contracts.clone();
        contracts.set_settlement_mode(migrator.settlement_mode());

        Ok(Output {
            initial_settlement_mode: SettlementModeResource(migrator.settlement_mode()),
            contracts: ContractsResource(contracts),
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
        if !self.dont_start {
            self.run_inner(stop_receiver.0).await
        } else {
            Ok(())
        }
    }
}
