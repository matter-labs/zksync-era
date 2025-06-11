use zksync_concurrency::{ctx, scope, sync};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
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
use zksync_node_sync::{node::ActionQueueSenderResource, ActionQueueSender};
use zksync_shared_resources::api::SyncState;
use zksync_state_keeper::{node::StateKeeperResource, StateKeeperBuilder};

/// Wiring layer for main node consensus component.
#[derive(Debug)]
pub struct MainNodeConsensusLayer {
    pub config: ConsensusConfig,
    pub secrets: ConsensusSecrets,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    sk: StateKeeperResource,
    action_queue_sender: ActionQueueSenderResource,
    sync_state: SyncState,
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

        let sync_state = input.sync_state;
        let action_queue_sender = input.action_queue_sender.0.take().ok_or_else(|| {
            WiringError::Configuration(
                "Action queue sender is taken by another resource".to_string(),
            )
        })?;

        let consensus_task = MainNodeConsensusTask {
            config: self.config,
            secrets: self.secrets,
            pool,
            sk: input.sk.0.take().unwrap(),
            sync_state,
            action_queue_sender,
        };

        Ok(Output { consensus_task })
    }
}

#[derive(Debug)]
pub struct MainNodeConsensusTask {
    config: ConsensusConfig,
    secrets: ConsensusSecrets,
    pool: ConnectionPool<Core>,
    sk: StateKeeperBuilder,
    sync_state: SyncState,
    action_queue_sender: ActionQueueSender,
}

#[async_trait::async_trait]
impl Task for MainNodeConsensusTask {
    fn id(&self) -> TaskId {
        "consensus".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let sk = self.sk.build(&stop_receiver.0).await.unwrap();
        let config = self.config;
        let secrets = self.secrets;
        let pool = self.pool;
        let action_queue_sender = self.action_queue_sender;
        let sync_state = self.sync_state;
        // We instantiate the root context here, since the consensus task is the only user of the
        // structured concurrency framework (`MainNodeConsensusTask` and `ExternalNodeTask` are considered mutually
        // exclusive).
        // Note, however, that awaiting for the `stop_receiver` is related to the root context behavior,
        // not the consensus task itself. There may have been any number of tasks running in the root context,
        // but we only need to wait for a stop request once, and it will be propagated to all child contexts.
        scope::run!(&ctx::root(), |ctx, s| async move {
            s.spawn_bg(crate::run_main_node(
                ctx,
                config,
                secrets,
                pool,
                sk,
                action_queue_sender,
                sync_state,
            ));
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
