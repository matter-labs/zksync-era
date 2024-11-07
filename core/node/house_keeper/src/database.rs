use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{system_dal::DatabaseMigration, ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};

use crate::periodic_job::PeriodicJob;

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
    pub polling_interval_ms: u64,
    pub connection_pool: ConnectionPool<Core>,
    pub database_health_updater: HealthUpdater,
}

#[async_trait]
impl PeriodicJob for DatabaseHealthTask {
    const SERVICE_NAME: &'static str = "DatabaseHealth";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self
            .connection_pool
            .connection_tagged("house_keeper")
            .await?;
        let last_migration = conn.system_dal().get_last_migration().await?;

        self.database_health_updater
            .update(DatabaseInfo { last_migration }.into());
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.polling_interval_ms
    }
}
