use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core, CoreDal,
};
use zksync_health_check::node::AppHealthCheckResource;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_di::api::{SyncState, SyncStateData};
use zksync_shared_metrics::EN_METRICS;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::EthNamespaceClient,
    node::MainNodeClientResource,
};

/// Wiring layer for [`SyncState`] maintenance.
/// If [`SyncStateResource`] is already provided by another layer, this layer does nothing.
#[derive(Debug)]
pub struct SyncStateUpdaterLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    /// Fetched to check whether the `SyncState` was already provided by another layer.
    pub sync_state: Option<SyncState>,
    pub app_health: AppHealthCheckResource,
    pub master_pool: PoolResource<MasterPool>,
    pub main_node_client: MainNodeClientResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    sync_state_metrics_task: SyncStateMetricsTask,
    pub sync_state: Option<SyncState>,
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
        if let Some(sync_state) = &input.sync_state {
            // `SyncState` was provided by some other layer -- we assume that the layer that added this resource
            // will be responsible for its maintenance.
            tracing::info!(
                "SyncState was provided by another layer, skipping SyncStateUpdaterLayer"
            );
            return Ok(Output {
                sync_state_metrics_task: SyncStateMetricsTask(sync_state.subscribe()),
                sync_state: None,
                sync_state_updater: None,
            });
        }

        let connection_pool = input.master_pool.get().await?;
        let MainNodeClientResource(main_node_client) = input.main_node_client;

        let sync_state = SyncState::default();
        let app_health = &input.app_health.0;
        app_health
            .insert_custom_component(Arc::new(sync_state.clone()))
            .map_err(WiringError::internal)?;

        Ok(Output {
            sync_state_metrics_task: SyncStateMetricsTask(sync_state.subscribe()),
            sync_state: Some(sync_state.clone()),
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

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        const UPDATE_INTERVAL: Duration = Duration::from_secs(10);

        while !*stop_receiver.0.borrow_and_update() {
            let local_block = self
                .connection_pool
                .connection()
                .await?
                .blocks_dal()
                .get_sealed_l2_block_number()
                .await?;

            let main_node_block = self.main_node_client.get_block_number().await?;

            if let Some(local_block) = local_block {
                self.sync_state.set_local_block(local_block);
                self.sync_state
                    .set_main_node_block(main_node_block.as_u32().into());
            }

            tokio::time::timeout(UPDATE_INTERVAL, stop_receiver.0.changed())
                .await
                .ok();
        }
        Ok(())
    }
}

/// Task updating sync state-related metrics.
#[derive(Debug)]
struct SyncStateMetricsTask(watch::Receiver<SyncStateData>);

#[async_trait]
impl Task for SyncStateMetricsTask {
    fn id(&self) -> TaskId {
        "sync_state_metrics_task".into()
    }

    async fn run(mut self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        while !*stop_receiver.0.borrow() {
            tokio::select! {
                _ = self.0.changed() => {
                    let data = self.0.borrow_and_update().clone();
                    let (is_synced, lag) = data.lag();
                    EN_METRICS.synced.set(is_synced.into());
                    if let Some(lag) = lag {
                        EN_METRICS.sync_lag.set(lag.into());
                    }
                }
                _ = stop_receiver.0.changed() => (),
            }
        }
        tracing::info!("Stop signal received, shutting down");
        Ok(())
    }
}
