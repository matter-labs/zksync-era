use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};

use crate::periodic_job::PeriodicJob;

/// This struct implements a static health check describing node's version information.
#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseInfo {
    last_migration: i64,
}

impl From<DatabaseInfo> for Health {
    fn from(details: DatabaseInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug)]
pub struct DatabaseHealthTask {
    pub connection_pool: ConnectionPool<Core>,
    pub database_health_updater: HealthUpdater,
}

impl DatabaseHealthTask {
    pub const POLLING_INTERVAL_MS: u64 = 10_000;
}

#[async_trait]
impl PeriodicJob for DatabaseHealthTask {
    const SERVICE_NAME: &'static str = "L1BatchMetricsReporter";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await.unwrap();
        let last_migration = conn.system_dal().get_last_migration().await.unwrap();

        self.database_health_updater
            .update(DatabaseInfo { last_migration }.into());
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        Self::POLLING_INTERVAL_MS
    }
}
