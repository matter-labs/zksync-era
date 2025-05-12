use anyhow::Context as _;
use zksync_concurrency::{ctx, scope, sync};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::{
    di::{MasterPool, PoolResource},
    ConnectionPool, Core,
};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_node_sync::{di::ActionQueueSenderResource, ActionQueueSender};
use zksync_shared_di::api::SyncState;
use zksync_web3_decl::{
    client::{DynClient, L2},
    di::MainNodeClientResource,
};

/// Wiring layer for external node consensus component.
#[derive(Debug)]
pub struct ExternalNodeConsensusLayer {
    pub build_version: semver::Version,
    pub config: Option<ConsensusConfig>,
    pub secrets: Option<ConsensusSecrets>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub sync_state: SyncState,
    pub action_queue_sender: ActionQueueSenderResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub consensus_task: ExternalNodeTask,
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

        let main_node_client = input.main_node_client.0;
        let sync_state = input.sync_state;
        let action_queue_sender = input.action_queue_sender.0.take().ok_or_else(|| {
            WiringError::Configuration(
                "Action queue sender is taken by another resource".to_string(),
            )
        })?;

        let config = match (self.config, self.secrets) {
            (Some(cfg), Some(secrets)) => Some((cfg, secrets)),
            (Some(_), None) => {
                return Err(WiringError::Configuration(
                    "Consensus config is specified, but secrets are missing".to_string(),
                ));
            }
            (None, _) => {
                // Secrets may be unconditionally embedded in some environments, but they are unused
                // unless a consensus config is provided.
                None
            }
        };

        let consensus_task = ExternalNodeTask {
            build_version: self.build_version,
            config,
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
    config: Option<(ConsensusConfig, ConsensusSecrets)>,
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
            s.spawn_bg(crate::era::run_external_node(
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
