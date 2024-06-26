use zksync_dal::{ConnectionPool, Core};
use zksync_node_sync::SyncState;
use zksync_web3_decl::client::{DynClient, L2};

use crate::{
    implementations::resources::{
        main_node_client::MainNodeClientResource,
        pools::{MasterPool, PoolResource},
        sync_state::SyncStateResource,
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for [`SyncState`] maintenance.
/// If [`SyncStateResource`] is already provided by another layer, this layer does nothing.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `MainNodeClientResource`
///
/// ## Adds resources
///
/// - `SyncStateResource`
///
/// ## Adds tasks
///
/// - `SyncStateUpdater`
#[derive(Debug)]
pub struct SyncStateUpdaterLayer;

#[async_trait::async_trait]
impl WiringLayer for SyncStateUpdaterLayer {
    fn layer_name(&self) -> &'static str {
        "sync_state_updater_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        if context.get_resource::<SyncStateResource>().is_ok() {
            // `SyncState` was provided by some other layer -- we assume that the layer that added this resource
            // will be responsible for its maintenance.
            tracing::info!(
                "SyncState was provided by another layer, skipping SyncStateUpdaterLayer"
            );
            return Ok(());
        }

        let pool = context.get_resource::<PoolResource<MasterPool>>()?;
        let MainNodeClientResource(main_node_client) = context.get_resource()?;

        let sync_state = SyncState::default();

        // Insert resource.
        context.insert_resource(SyncStateResource(sync_state.clone()))?;

        // Insert task
        context.add_task(Box::new(SyncStateUpdater {
            sync_state,
            connection_pool: pool.get().await?,
            main_node_client,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct SyncStateUpdater {
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
