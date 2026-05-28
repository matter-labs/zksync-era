use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core, CoreDal,
};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_metrics::EN_METRICS;
use zksync_shared_resources::api::{SyncState, SyncStateData};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::ClientRpcContext,
    namespaces::EthNamespaceClient,
};

/// Wiring layer for [`SyncState`] maintenance.
/// If [`SyncStateResource`] is already provided by another layer, this layer does nothing.
#[derive(Debug)]
pub struct SyncStateUpdaterLayer;

#[derive(Debug, FromContext)]
pub struct Input {
    /// Fetched to check whether the `SyncState` was already provided by another layer.
    sync_state: Option<SyncState>,
    app_health: Arc<AppHealthCheck>,
    master_pool: PoolResource<MasterPool>,
    main_node_client: Box<DynClient<L2>>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    sync_state_metrics_task: SyncStateMetricsTask,
    sync_state: Option<SyncState>,
    #[context(task)]
    sync_state_updater: Option<SyncStateUpdater>,
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

        let sync_state = SyncState::default();
        input
            .app_health
            .insert_custom_component(Arc::new(sync_state.clone()))
            .map_err(WiringError::internal)?;

        Ok(Output {
            sync_state_metrics_task: SyncStateMetricsTask(sync_state.subscribe()),
            sync_state: Some(sync_state.clone()),
            sync_state_updater: Some(SyncStateUpdater {
                sync_state,
                connection_pool,
                main_node_client: input.main_node_client,
                update_interval: SyncStateUpdater::DEFAULT_UPDATE_INTERVAL,
            }),
        })
    }
}

#[derive(Debug)]
pub struct SyncStateUpdater {
    sync_state: SyncState,
    connection_pool: ConnectionPool<Core>,
    main_node_client: Box<DynClient<L2>>,
    update_interval: Duration,
}

impl SyncStateUpdater {
    const DEFAULT_UPDATE_INTERVAL: Duration = Duration::from_secs(10);

    async fn step(&self) -> anyhow::Result<()> {
        let local_block = self
            .connection_pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await?;
        let Some(local_block) = local_block else {
            // No local blocks yet, no need to query the main node.
            return Ok(());
        };

        let fetch_result = self
            .main_node_client
            .get_block_number()
            .rpc_context("get_block_number")
            .await;
        let main_node_block = match fetch_result {
            Ok(block) => block,
            Err(err) if err.is_retryable() => {
                tracing::warn!(%err, "Failed fetching latest block number from main node, will retry after {:?}", self.update_interval);
                return Ok(());
            }
            Err(err) => {
                return Err(err).context("Fatal error fetching latest block number from main node");
            }
        };

        self.sync_state.set_local_block(local_block);
        self.sync_state
            .set_main_node_block(main_node_block.as_u32().into());
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for SyncStateUpdater {
    fn id(&self) -> TaskId {
        "sync_state_updater".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        while !*stop_receiver.0.borrow_and_update() {
            self.step().await?;
            tokio::time::timeout(self.update_interval, stop_receiver.0.changed())
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
                    let data = *self.0.borrow_and_update();
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_types::{L2BlockNumber, U64};
    use zksync_web3_decl::{client::MockClient, jsonrpsee::core::ClientError};

    use super::*;

    #[tokio::test]
    async fn sync_state_updater_basics() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let sync_state = SyncState::default();
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_blockNumber", move || Ok(U64::from(42)))
            .build();
        let (stop_sender, stop_receiver) = watch::channel(false);

        let updater = Box::new(SyncStateUpdater {
            sync_state: sync_state.clone(),
            connection_pool: pool.clone(),
            main_node_client: Box::new(main_node_client),
            update_interval: Duration::from_millis(10),
        });
        let task_handle = tokio::spawn(updater.run(StopReceiver(stop_receiver)));

        let sync_state = *sync_state
            .subscribe()
            .wait_for(|state| state.main_node_block().is_some() && state.local_block().is_some())
            .await
            .unwrap();

        assert!(!task_handle.is_finished());
        assert_eq!(sync_state.local_block(), Some(L2BlockNumber(0)));
        assert_eq!(sync_state.main_node_block(), Some(L2BlockNumber(42)));

        // Check graceful shutdown.
        stop_sender.send_replace(true);
        task_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn sync_state_updater_handles_transient_errors() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();

        let sync_state = SyncState::default();
        let request_count = AtomicUsize::new(0);
        let main_node_client = MockClient::builder(L2::default())
            .method("eth_blockNumber", move || {
                // Emulate transient connectivity issues
                if request_count.fetch_add(1, Ordering::Relaxed) < 10 {
                    Err(ClientError::RequestTimeout)
                } else {
                    Ok(U64::from(1))
                }
            })
            .build();
        let (_stop_sender, stop_receiver) = watch::channel(false);

        let updater = Box::new(SyncStateUpdater {
            sync_state: sync_state.clone(),
            connection_pool: pool.clone(),
            main_node_client: Box::new(main_node_client),
            update_interval: Duration::from_millis(10),
        });
        let task_handle = tokio::spawn(updater.run(StopReceiver(stop_receiver)));

        let sync_state = *sync_state
            .subscribe()
            .wait_for(|state| state.main_node_block().is_some() && state.local_block().is_some())
            .await
            .unwrap();

        assert!(!task_handle.is_finished());
        assert_eq!(sync_state.local_block(), Some(L2BlockNumber(0)));
        assert_eq!(sync_state.main_node_block(), Some(L2BlockNumber(1)));
    }
}
