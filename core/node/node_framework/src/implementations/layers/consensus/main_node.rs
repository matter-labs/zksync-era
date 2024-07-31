use zksync_concurrency::{ctx, scope};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::{ConnectionPool, Core};
use zksync_node_consensus as consensus;
use zksync_node_framework_derive::FromContext;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer for main node consensus component.
#[derive(Debug)]
pub struct MainNodeConsensusLayer {
    pub config: ConsensusConfig,
    pub secrets: ConsensusSecrets,
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
    pub consensus_task: MainNodeConsensusTask,
}

#[async_trait::async_trait]
impl WiringLayer for MainNodeConsensusLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "main_node_consensus_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;

        let consensus_task = MainNodeConsensusTask {
            config: self.config,
            secrets: self.secrets,
            pool,
        };

        Ok(Output { consensus_task })
    }
}

#[derive(Debug)]
pub struct MainNodeConsensusTask {
    config: ConsensusConfig,
    secrets: ConsensusSecrets,
    pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for MainNodeConsensusTask {
    fn id(&self) -> TaskId {
        "consensus".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // We instantiate the root context here, since the consensus task is the only user of the
        // structured concurrency framework (`MainNodeConsensusTask` and `ExternalNodeTask` are considered mutually
        // exclusive).
        // Note, however, that awaiting for the `stop_receiver` is related to the root context behavior,
        // not the consensus task itself. There may have been any number of tasks running in the root context,
        // but we only need to wait for stop signal once, and it will be propagated to all child contexts.
        let root_ctx = ctx::root();
        scope::run!(&root_ctx, |ctx, s| async move {
            s.spawn_bg(consensus::era::run_main_node(
                ctx,
                self.config,
                self.secrets,
                self.pool,
            ));
            let _ = stop_receiver.0.wait_for(|stop| *stop).await?;
            Ok(())
        })
        .await
    }
}
