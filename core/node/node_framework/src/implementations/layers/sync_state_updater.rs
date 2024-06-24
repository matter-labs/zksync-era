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

/// Runs the dynamic sync state updater for `SyncState` if no `SyncState` was provided before.
/// This layer may be used as a fallback for EN API if API server runs without the core component.
#[derive(Debug)]
pub struct SyncStateUpdaterLayer;

#[async_trait::async_trait]
impl WiringLayer for SyncStateUpdaterLayer {
    fn layer_name(&self) -> &'static str {
        "sync_state_updater_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        if context.get_resource::<SyncStateResource>().await.is_ok() {
            // `SyncState` was provided by some other layer -- we assume that the layer that added this resource
            // will be responsible for its maintenance.
            tracing::info!(
                "SyncState was provided by another layer, skipping SyncStateUpdaterLayer"
            );
            return Ok(());
        }

        let pool = context.get_resource::<PoolResource<MasterPool>>().await?;
        let MainNodeClientResource(main_node_client) = context.get_resource().await?;

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
