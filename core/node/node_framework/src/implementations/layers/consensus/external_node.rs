use anyhow::Context as _;
use zksync_concurrency::{ctx, scope, sync};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::{ConnectionPool, Core};
use zksync_node_consensus as consensus;
use zksync_node_framework_derive::IntoContext;
use zksync_node_sync::{ActionQueueSender, SyncState};
use zksync_web3_decl::client::{DynClient, L2};

use crate::{
    implementations::resources::{
        action_queue::ActionQueueSenderResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        sync_state::SyncStateResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};

/// Wiring layer for external node consensus component.
#[derive(Debug)]
pub struct ExternalNodeConsensusLayer {
    pub config: Option<ConsensusConfig>,
    pub secrets: Option<ConsensusSecrets>,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
    pub sync_state: SyncStateResource,
    pub action_queue_sender: ActionQueueSenderResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
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
        let sync_state = input.sync_state.0;
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
        // but we only need to wait for stop signal once, and it will be propagated to all child contexts.
        scope::run!(&ctx::root(), |ctx, s| async {
            s.spawn_bg(consensus::era::run_external_node(
                ctx,
                self.config,
                self.pool,
                self.sync_state,
                self.main_node_client,
                self.action_queue_sender,
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
