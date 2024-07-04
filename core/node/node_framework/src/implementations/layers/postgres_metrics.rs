use std::time::Duration;

use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core};

use crate::{
    implementations::resources::pools::{PoolResource, ReplicaPool},
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

const SCRAPE_INTERVAL: Duration = Duration::from_secs(60);

/// Wiring layer for the Postgres metrics exporter.
#[derive(Debug)]
pub struct PostgresMetricsLayer;

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: PostgresMetricsScrapingTask,
}

#[async_trait::async_trait]
impl WiringLayer for PostgresMetricsLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "postgres_metrics_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool_for_metrics = input.replica_pool.get_singleton().await?;
        let task = PostgresMetricsScrapingTask { pool_for_metrics };

        Ok(Output { task })
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
