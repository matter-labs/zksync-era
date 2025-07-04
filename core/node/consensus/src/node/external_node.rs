use anyhow::Context as _;
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
use zksync_web3_decl::client::{DynClient, L2};

/// Wiring layer for external node consensus component.
#[derive(Debug)]
pub struct ExternalNodeConsensusLayer {
    pub build_version: semver::Version,
    pub config: ConsensusConfig,
    pub secrets: ConsensusSecrets,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
    sync_state: SyncState,
    action_queue_sender: ActionQueueSenderResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    consensus_task: ExternalNodeTask,
}

#[async_trait::async_trait]
impl WiringLayer for ExternalNodeConsensusLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_node_consensus_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;

        let main_node_client = input.main_node_client;
        let sync_state = input.sync_state;
        let action_queue_sender = input.action_queue_sender.0.take().ok_or_else(|| {
            WiringError::Configuration(
                "Action queue sender is taken by another resource".to_string(),
            )
        })?;

        let consensus_task = ExternalNodeTask {
            build_version: self.build_version,
            config: (self.config, self.secrets),
            pool,
            main_node_client,
            sync_state,
            action_queue_sender,
        };
        Ok(Output { consensus_task })
    }
}

#[derive(Debug)]
pub struct ExternalNodeTask {
    build_version: semver::Version,
    config: (ConsensusConfig, ConsensusSecrets),
    pool: ConnectionPool<Core>,
    main_node_client: Box<DynClient<L2>>,
    sync_state: SyncState,
    action_queue_sender: ActionQueueSender,
}

#[async_trait::async_trait]
impl Task for ExternalNodeTask {
    fn id(&self) -> TaskId {
        "consensus_fetcher".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // We instantiate the root context here, since the consensus task is the only user of the
        // structured concurrency framework (`MainNodeConsensusTask` and `ExternalNodeTask` are considered mutually
        // exclusive).
        // Note, however, that awaiting for the `stop_receiver` is related to the root context behavior,
        // not the consensus task itself. There may have been any number of tasks running in the root context,
        // but we only need to wait for a stop request once, and it will be propagated to all child contexts.
        scope::run!(&ctx::root(), |ctx, s| async {
            s.spawn_bg(crate::run_external_node(
                ctx,
                self.config,
                self.pool,
                self.sync_state,
                self.main_node_client,
                self.action_queue_sender,
                self.build_version,
            ));
            // `run_external_node` might return an error or panic,
            // in which case we need to return immediately,
            // rather than wait for the `stop_receiver`.
            let _ = sync::wait_for(ctx, &mut stop_receiver.0, |stop| *stop).await;
            Ok(())
        })
        .await
        .context("consensus actor")
    }
}
