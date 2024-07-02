use std::time::Duration;

use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core};

use crate::{
    implementations::resources::pools::{PoolResource, ReplicaPool},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
};

const SCRAPE_INTERVAL: Duration = Duration::from_secs(60);

/// Wiring layer for the Postgres metrics exporter.
///
/// ## Requests resources
///
/// - `PoolResource<ReplicaPool>`
///
/// ## Adds tasks
///
/// - `PostgresMetricsScrapingTask`
#[derive(Debug)]
pub struct PostgresMetricsLayer;

#[async_trait::async_trait]
impl WiringLayer for PostgresMetricsLayer {
    fn layer_name(&self) -> &'static str {
        "postgres_metrics_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let replica_pool_resource = context.get_resource::<PoolResource<ReplicaPool>>()?;
        let pool_for_metrics = replica_pool_resource.get_singleton().await?;
        context.add_task(PostgresMetricsScrapingTask { pool_for_metrics });

        Ok(())
    }
}

#[derive(Debug)]
struct PostgresMetricsScrapingTask {
    pool_for_metrics: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for PostgresMetricsScrapingTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "postgres_metrics_scraping".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tokio::select! {
            () = PostgresMetrics::run_scraping(self.pool_for_metrics, SCRAPE_INTERVAL) => {
                tracing::warn!("Postgres metrics scraping unexpectedly stopped");
            }
            _ = stop_receiver.0.changed() => {
                tracing::info!("Stop signal received, Postgres metrics scraping is shutting down");
            }
        }
        Ok(())
    }
}
