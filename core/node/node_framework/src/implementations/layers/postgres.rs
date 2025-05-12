use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::RwLock;
use zksync_dal::{
    metrics::PostgresMetrics, system_dal::DatabaseMigration, ConnectionPool, Core, CoreDal,
};
use zksync_health_check::{CheckHealth, Health, HealthStatus};

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

/// Execution interval for Postrgres metrics and healthcheck tasks
const TASK_EXECUTION_INTERVAL: Duration = Duration::from_secs(60);

/// Wiring layer for the Postgres metrics exporter and healthcheck.
#[derive(Debug)]
pub struct PostgresLayer;

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
        app_health
            .insert_custom_component(Arc::new(DatabaseHealthCheck {
                polling_interval: TASK_EXECUTION_INTERVAL,
                pool,
                cached: RwLock::default(),
            }))
            .map_err(WiringError::internal)?;

        Ok(Output { metrics_task })
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
            () = PostgresMetrics::run_scraping(self.pool_for_metrics, TASK_EXECUTION_INTERVAL) => {
                tracing::warn!("Postgres metrics scraping unexpectedly stopped");
            }
            _ = stop_receiver.0.changed() => {
                tracing::info!("Stop request received, Postgres metrics scraping is shutting down");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseInfo {
    last_migration: DatabaseMigration,
}

impl From<DatabaseInfo> for Health {
    fn from(details: DatabaseInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug)]
struct DatabaseHealthCheck {
    polling_interval: Duration,
    pool: ConnectionPool<Core>,
    cached: RwLock<Option<(DatabaseInfo, Instant)>>,
}

impl DatabaseHealthCheck {
    async fn update(&self) -> anyhow::Result<DatabaseInfo> {
        let mut conn = self.pool.connection_tagged("postgres_healthcheck").await?;
        let last_migration = conn.system_dal().get_last_migration().await?;
        Ok(DatabaseInfo { last_migration })
    }

    fn validate_cache(&self, cache: Option<&(DatabaseInfo, Instant)>) -> Option<DatabaseInfo> {
        let now = Instant::now();
        if let Some((cached, cached_at)) = cache {
            let elapsed = now
                .checked_duration_since(*cached_at)
                .unwrap_or(Duration::ZERO);
            (elapsed <= self.polling_interval).then(|| cached.clone())
        } else {
            None
        }
    }
}

#[async_trait]
impl CheckHealth for DatabaseHealthCheck {
    fn name(&self) -> &'static str {
        "database"
    }

    // If the DB malfunctions, this method would time out, which would lead to the health check marked as failed.
    async fn check_health(&self) -> Health {
        let cached = self.cached.read().await.clone();
        if let Some(cache) = self.validate_cache(cached.as_ref()) {
            return cache.into();
        }

        let mut cached_lock = self.cached.write().await;
        // The cached value may have been updated by another task.
        if let Some(cache) = self.validate_cache(cached_lock.as_ref()) {
            return cache.into();
        }

        match self.update().await {
            Ok(info) => {
                *cached_lock = Some((info.clone(), Instant::now()));
                info.into()
            }
            Err(err) => {
                tracing::warn!("Error updating database health: {err:#}");
                cached.map_or_else(|| HealthStatus::Affected.into(), |(info, _)| info.into())
            }
        }
    }
}
