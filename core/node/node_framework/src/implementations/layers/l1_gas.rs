use std::sync::Arc;

use anyhow::Context;
use zksync_config::{
    configs::{chain::StateKeeperConfig, eth_sender::PubdataSendingMode},
    GasAdjusterConfig, GenesisConfig,
};
use zksync_node_fee_model::{l1_gas_price::GasAdjuster, MainNodeFeeInputProvider};
use zksync_types::fee_model::FeeModelConfig;

use crate::{
    implementations::resources::{
        eth_interface::EthInterfaceResource, fee_input::FeeInputResource,
        l1_tx_params::L1TxParamsResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct SequencerL1GasLayer {
    gas_adjuster_config: GasAdjusterConfig,
    genesis_config: GenesisConfig,
    pubdata_sending_mode: PubdataSendingMode,
    state_keeper_config: StateKeeperConfig,
}

impl SequencerL1GasLayer {
    pub fn new(
        gas_adjuster_config: GasAdjusterConfig,
        genesis_config: GenesisConfig,
        state_keeper_config: StateKeeperConfig,
        pubdata_sending_mode: PubdataSendingMode,
    ) -> Self {
        Self {
            gas_adjuster_config,
            genesis_config,
            pubdata_sending_mode,
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for SequencerL1GasLayer {
    fn layer_name(&self) -> &'static str {
        "sequencer_l1_gas_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let client = context.get_resource::<EthInterfaceResource>().await?.0;
        let adjuster = GasAdjuster::new(
            client,
            self.gas_adjuster_config,
            self.pubdata_sending_mode,
            self.genesis_config.l1_batch_commit_data_generator_mode,
            None, // TODO: Add connection pool
            None, // TODO: Add base token config
        )
        .await
        .context("GasAdjuster::new()")?;
        let gas_adjuster = Arc::new(adjuster);

        let batch_fee_input_provider = Arc::new(MainNodeFeeInputProvider::new(
            gas_adjuster.clone(),
            FeeModelConfig::from_state_keeper_config(&self.state_keeper_config),
        ));
        context.insert_resource(FeeInputResource(batch_fee_input_provider))?;

        context.insert_resource(L1TxParamsResource(gas_adjuster.clone()))?;

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
    fn id(&self) -> TaskId {
        "gas_adjuster".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.gas_adjuster.run(stop_receiver.0).await
    }
}
