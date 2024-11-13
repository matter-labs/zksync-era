use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use zksync_dal::{
    metrics::PostgresMetrics, system_dal::DatabaseMigration, ConnectionPool, Core, CoreDal,
};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{PoolResource, ReplicaPool},
    },
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

const SCRAPE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Config {
    pub polling_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            polling_interval_ms: 10_000,
        }
    }
}

/// Wiring layer for the Postgres metrics exporter.
#[derive(Debug, Default)]
pub struct PostgresLayer {
    config: Config,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub metrics_task: PostgresMetricsScrapingTask,
    #[context(task)]
    pub health_task: DatabaseHealthTask,
}

#[async_trait::async_trait]
impl WiringLayer for PostgresLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "postgres_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.replica_pool.get().await?;
        let metrics_task = PostgresMetricsScrapingTask {
            pool_for_metrics: pool.clone(),
        };

        let app_health = input.app_health.0;
        let (database_health_check, updater) = ReactiveHealthCheck::new("database");

        app_health
            .insert_component(database_health_check)
            .map_err(WiringError::internal)?;

        let health_task = DatabaseHealthTask {
            polling_interval_ms: self.config.polling_interval_ms,
            connection_pool: pool,
            updater,
        };

        Ok(Output {
            metrics_task,
            health_task,
        })
    }
}

#[derive(Debug)]
pub struct PostgresMetricsScrapingTask {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseInfo {
    last_migration: DatabaseMigration,
}

impl From<DatabaseInfo> for Health {
    fn from(details: DatabaseInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug)]
pub struct DatabaseHealthTask {
    polling_interval_ms: u64,
    connection_pool: ConnectionPool<Core>,
    updater: HealthUpdater,
}

impl DatabaseHealthTask {
    async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let timeout = Duration::from_millis(self.polling_interval_ms);
        let mut conn = self
            .connection_pool
            .connection_tagged("postgres_healthcheck")
            .await?;

        tracing::info!("Starting database healthcheck with frequency: {timeout:?}",);

        while !*stop_receiver.borrow_and_update() {
            let last_migration = conn.system_dal().get_last_migration().await?;
            self.updater.update(DatabaseInfo { last_migration }.into());

            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(timeout, stop_receiver.changed())
                .await
                .ok();
        }
        tracing::info!("Stop signal received; database healthcheck is shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for DatabaseHealthTask {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "database_health".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
