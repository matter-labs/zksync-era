use anyhow::Context as _;
use zksync_concurrency::{ctx, scope};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::{ConnectionPool, Core};
use zksync_node_consensus as consensus;
use zksync_node_sync::{ActionQueueSender, SyncState};
use zksync_web3_decl::client::{DynClient, L2};

use crate::{
    implementations::resources::{
        action_queue::ActionQueueSenderResource,
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        sync_state::SyncStateResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug, Copy, Clone)]
pub enum Mode {
    Main,
    External,
}

/// Wiring layer for consensus component.
/// Can work in either "main" or "external" mode.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `MainNodeClientResource` (if `Mode::External`)
/// - `SyncStateResource` (if `Mode::External`)
/// - `ActionQueueSenderResource` (if `Mode::External`)
///
/// ## Adds tasks
///
/// - `MainNodeConsensusTask` (if `Mode::Main`)
/// - `FetcherTask` (if `Mode::External`)
#[derive(Debug)]
pub struct ConsensusLayer {
    pub mode: Mode,
    pub config: Option<ConsensusConfig>,
    pub secrets: Option<ConsensusSecrets>,
}

#[async_trait::async_trait]
impl WiringLayer for ConsensusLayer {
    fn layer_name(&self) -> &'static str {
        "consensus_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool = context
            .get_resource::<PoolResource<MasterPool>>()?
            .get()
            .await?;

        match self.mode {
            Mode::Main => {
                let config = self.config.ok_or_else(|| {
                    WiringError::Configuration("Missing public consensus config".to_string())
                })?;
                let secrets = self.secrets.ok_or_else(|| {
                    WiringError::Configuration("Missing private consensus config".to_string())
                })?;
                let task = MainNodeConsensusTask {
                    config,
                    secrets,
                    pool,
                };
                context.add_task(task);
            }
            Mode::External => {
                let main_node_client = context.get_resource::<MainNodeClientResource>()?.0;
                let sync_state = context.get_resource::<SyncStateResource>()?.0;
                let action_queue_sender = context
                    .get_resource::<ActionQueueSenderResource>()?
                    .0
                    .take()
                    .ok_or_else(|| {
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

                let task = FetcherTask {
                    config,
                    pool,
                    main_node_client,
                    sync_state,
                    action_queue_sender,
                };
                context.add_task(task);
            }
        }
        Ok(())
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
        // structured concurrency framework (`MainNodeConsensusTask` and `FetcherTask` are considered mutually
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

#[derive(Debug)]
pub struct FetcherTask {
    config: Option<(ConsensusConfig, ConsensusSecrets)>,
    pool: ConnectionPool<Core>,
    main_node_client: Box<DynClient<L2>>,
    sync_state: SyncState,
    action_queue_sender: ActionQueueSender,
}

#[async_trait::async_trait]
impl Task for FetcherTask {
    fn id(&self) -> TaskId {
        "consensus_fetcher".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // We instantiate the root context here, since the consensus task is the only user of the
        // structured concurrency framework (`MainNodeConsensusTask` and `FetcherTask` are considered mutually
        // exclusive).
        // Note, however, that awaiting for the `stop_receiver` is related to the root context behavior,
        // not the consensus task itself. There may have been any number of tasks running in the root context,
        // but we only need to wait for stop signal once, and it will be propagated to all child contexts.
        let root_ctx = ctx::root();
        scope::run!(&root_ctx, |ctx, s| async {
            s.spawn_bg(consensus::era::run_en(
                ctx,
                self.config,
                self.pool,
                self.sync_state,
                self.main_node_client,
                self.action_queue_sender,
            ));
            let _ = stop_receiver.0.wait_for(|stop| *stop).await?;
            Ok(())
        })
        .await
        .context("consensus actor")
    }
}
