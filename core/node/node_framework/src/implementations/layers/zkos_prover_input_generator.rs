use std::sync::Arc;
use zksync_node_framework_derive::{FromContext, IntoContext};
use zksync_state::{AsyncCatchupTask, KeepUpdatedTask};
use zksync_zkos_prover_input_generator::ZkosProverInputGenerator;
use zksync_zkos_state_keeper::ZkosStateKeeper;
use crate::implementations::resources::pools::{MasterPool, PoolResource};
use crate::{StopReceiver, Task, TaskId, WiringError, WiringLayer};
use crate::implementations::layers::zkos_state_keeper::ZkOsStateKeeperTask;
use crate::implementations::resources::fee_input::SequencerFeeInputResource;
use crate::implementations::resources::state_keeper::{ZkOsConditionalSealerResource, ZkOsOutputHandlerResource, ZkOsStateKeeperIOResource};
use crate::service::ShutdownHook;

#[derive(Debug)]
pub struct ZkOsProverInputGeneratorLayer;


impl ZkOsProverInputGeneratorLayer {
    pub fn new() -> Self {
        Self {
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub prover_input_generator_task: ZkOsProverInputGeneratorTask,
}

#[derive(Debug)]
pub struct ZkOsProverInputGeneratorTask {
    pool: PoolResource<MasterPool>,
}

#[async_trait::async_trait]
impl WiringLayer for ZkOsProverInputGeneratorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "zk_os_prover_input_generator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let prover_input_generator_task = ZkOsProverInputGeneratorTask {
            pool: input.master_pool.clone(),
        };
        Ok(Output {
            prover_input_generator_task,
        })
    }
}


#[async_trait::async_trait]
impl Task for ZkOsProverInputGeneratorTask {
    fn id(&self) -> TaskId {
        "zkos_prover_input_generator_task".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let zkos_prover_input_generator = ZkosProverInputGenerator::new(
            stop_receiver.0,
            self.pool.get().await?,
        );
        zkos_prover_input_generator.run().await
    }
}
