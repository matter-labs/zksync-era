use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};
use zksync_types::L2BlockNumber;

use crate::periodic_job::PeriodicJob;

#[derive(Debug, Serialize, Deserialize)]
pub struct StateKeeperInfo {
    last_miniblock_protocol_upgrade: Option<()>,
    last_miniblock: Option<L2BlockNumber>,
    batch_number: Option<()>,
}

impl From<StateKeeperInfo> for Health {
    fn from(details: StateKeeperInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug)]
pub struct StateKeeperHealthTask {
    pub connection_pool: ConnectionPool<Core>,
    pub state_keeper_health_updater: HealthUpdater,
}

impl StateKeeperHealthTask {
    pub const POLLING_INTERVAL_MS: u64 = 10_000;
}

#[async_trait]
impl PeriodicJob for StateKeeperHealthTask {
    const SERVICE_NAME: &'static str = "StateKeeperHealth";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await.unwrap();
        let last_miniblock = conn
            .blocks_web3_dal()
            .get_last_miniblock_number()
            .await
            .unwrap();

        self.state_keeper_health_updater.update(
            StateKeeperInfo {
                last_miniblock_protocol_upgrade: None,
                last_miniblock,
                batch_number: None,
            }
            .into(),
        );
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        Self::POLLING_INTERVAL_MS
    }
}
