use zksync_concurrency::{ctx, scope, sync};
use zksync_config::configs::consensus::ConsensusConfig;
use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core,
};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for main node consensus component.
#[derive(Debug)]
pub struct MainNodeConsensusLayer {
    pub config: ConsensusConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    consensus_task: MainNodeConsensusTask,
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
            pool,
        };

        Ok(Output { consensus_task })
    }
}

#[derive(Debug)]
pub struct MainNodeConsensusTask {
    config: ConsensusConfig,
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
        // but we only need to wait for a stop request once, and it will be propagated to all child contexts.
        scope::run!(&ctx::root(), |ctx, s| async move {
            s.spawn_bg(crate::run_main_node(ctx, self.config, self.pool));
            // `run_main_node` might return an error or panic,
            // in which case we need to return immediately,
            // rather than wait for the `stop_receiver`.
            // We ignore the `Canceled` error and return `Ok()` instead,
            // because cancellation is expected.
            let _ = sync::wait_for(ctx, &mut stop_receiver.0, |stop| *stop).await;
            Ok(())
        })
        .await
    }
}
