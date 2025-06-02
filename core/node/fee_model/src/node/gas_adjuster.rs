use std::sync::Arc;

use anyhow::Context;
use zksync_config::{GasAdjusterConfig, GenesisConfig};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::PubdataSendingModeResource;
use zksync_web3_decl::node::SettlementLayerClient;

use crate::l1_gas_price::GasAdjuster;

/// Wiring layer for sequencer L1 gas interfaces.
/// Adds several resources that depend on L1 gas price.
#[derive(Debug)]
pub struct GasAdjusterLayer {
    gas_adjuster_config: GasAdjusterConfig,
    genesis_config: GenesisConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    client: SettlementLayerClient,
    pubdata_sending_mode: PubdataSendingModeResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    gas_adjuster: Arc<GasAdjuster>,
    /// Only runs if someone uses the resources listed above.
    #[context(task)]
    gas_adjuster_task: GasAdjusterTask,
}

impl GasAdjusterLayer {
    pub fn new(gas_adjuster_config: GasAdjusterConfig, genesis_config: GenesisConfig) -> Self {
        Self {
            gas_adjuster_config,
            genesis_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for GasAdjusterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "gas_adjuster_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let client = match input.client {
            SettlementLayerClient::L1(client) => client.into(),
            SettlementLayerClient::Gateway(client) => client.into(),
        };

        let adjuster = GasAdjuster::new(
            client,
            self.gas_adjuster_config,
            input.pubdata_sending_mode.0,
            self.genesis_config.l1_batch_commit_data_generator_mode,
        )
        .await
        .context("GasAdjuster::new()")?;
        let gas_adjuster = Arc::new(adjuster);

        Ok(Output {
            gas_adjuster: gas_adjuster.clone(),
            gas_adjuster_task: GasAdjusterTask { gas_adjuster },
        })
    }
}

#[derive(Debug)]
pub struct GasAdjusterTask {
    gas_adjuster: Arc<GasAdjuster>,
}

#[async_trait::async_trait]
impl Task for GasAdjusterTask {
    fn id(&self) -> TaskId {
        "gas_adjuster".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Gas adjuster layer is added to provide a resource for anyone to use, but it comes with
        // a support task. If nobody has used the resource, we don't need to run the support task.
        if Arc::strong_count(&self.gas_adjuster) == 1 {
            tracing::info!(
                "Gas adjuster is not used by any other task, not running the support task"
            );
            stop_receiver.0.changed().await?;
            return Ok(());
        }

        self.gas_adjuster.run(stop_receiver.0).await
    }
}
