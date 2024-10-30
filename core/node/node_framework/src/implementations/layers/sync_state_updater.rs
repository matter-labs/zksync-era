use zksync_dal::{ConnectionPool, Core};
use zksync_node_sync::SyncState;
use zksync_web3_decl::client::{DynClient, L2};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        sync_state::SyncStateResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for [`SyncState`] maintenance.
/// If [`SyncStateResource`] is already provided by another layer, this layer does nothing.
#[derive(Debug)]
pub struct SyncStateUpdaterLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    /// Fetched to check whether the `SyncState` was already provided by another layer.
    pub sync_state: Option<SyncStateResource>,
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub sync_state: Option<SyncStateResource>,
    #[context(task)]
    pub sync_state_updater: Option<SyncStateUpdater>,
}

#[async_trait::async_trait]
impl WiringLayer for SyncStateUpdaterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "sync_state_updater_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        if input.sync_state.is_some() {
            // `SyncState` was provided by some other layer -- we assume that the layer that added this resource
            // will be responsible for its maintenance.
            tracing::info!(
                "SyncState was provided by another layer, skipping SyncStateUpdaterLayer"
            );
            return Ok(Output {
                sync_state: None,
                sync_state_updater: None,
            });
        }

        let connection_pool = input.master_pool.get().await?;
        let MainNodeClientResource(main_node_client) = input.main_node_client;

        let sync_state = SyncState::default();

        Ok(Output {
            sync_state: Some(sync_state.clone().into()),
            sync_state_updater: Some(SyncStateUpdater {
                sync_state,
                connection_pool,
                main_node_client,
            }),
        })
    }
}

#[derive(Debug)]
pub struct SyncStateUpdater {
    sync_state: SyncState,
    connection_pool: ConnectionPool<Core>,
    main_node_client: Box<DynClient<L2>>,
}

#[async_trait::async_trait]
impl Task for SyncStateUpdater {
    fn id(&self) -> TaskId {
        "sync_state_updater".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.sync_state
            .run_updater(self.connection_pool, self.main_node_client, stop_receiver.0)
            .await?;
        Ok(())
    }
}
