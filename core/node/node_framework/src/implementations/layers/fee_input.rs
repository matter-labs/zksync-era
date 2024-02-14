use std::sync::Arc;

use anyhow::Context;
use zksync_config::{configs::chain::StateKeeperConfig, ETHClientConfig, GasAdjusterConfig};
use zksync_core::{fee_model::MainNodeFeeInputProvider, l1_gas_price::GasAdjuster};
use zksync_eth_client::clients::QueryClient;
use zksync_types::fee_model::FeeModelConfig;

use crate::{
    implementations::resources::fee_input::FeeInputResource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct SequencerFeeInputLayer {
    gas_adjuster_config: GasAdjusterConfig,
    eth_client_config: ETHClientConfig,
    state_keeper_config: StateKeeperConfig,
}

impl SequencerFeeInputLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        eth_client_config: ETHClientConfig,
        state_keeper_config: StateKeeperConfig,
    ) -> Self {
        Self {
            gas_adjuster_config,
            eth_client_config,
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for SequencerFeeInputLayer {
    fn layer_name(&self) -> &'static str {
        "sequencer_fee_input_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let query_client =
            QueryClient::new(&self.eth_client_config.web3_url).context("QueryClient::new()")?;
        let adjuster = GasAdjuster::new(Arc::new(query_client.clone()), self.gas_adjuster_config)
            .await
            .context("GasAdjuster::new()")?;
        let gas_adjuster = Arc::new(adjuster);

        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            gas_adjuster.clone(),
            FeeModelConfig::from_state_keeper_config(&self.state_keeper_config),
        ));
        context.add_resource(FeeInputResource(batch_fee_input_provider))?;

        context.add_task(Box::new(GasAdjusterTask { gas_adjuster }));
        Ok(())
    }
}

#[derive(Debug)]
struct GasAdjusterTask {
    gas_adjuster: Arc<GasAdjuster>,
}

#[async_trait::async_trait]
impl Task for GasAdjusterTask {
    fn name(&self) -> &'static str {
        "gas_adjuster"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.gas_adjuster.run(stop_receiver.0).await
    }
}
