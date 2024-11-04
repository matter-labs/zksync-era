use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};
use zksync_types::{block::L2BlockHeader, L2BlockNumber, ProtocolVersionId};

use crate::periodic_job::PeriodicJob;

#[derive(Debug, Serialize, Deserialize)]
pub struct L2BlockHeaderInfo {
    pub number: L2BlockNumber,
    pub timestamp: u64,
    pub protocol_version: Option<ProtocolVersionId>,
}

impl From<L2BlockHeader> for L2BlockHeaderInfo {
    fn from(header: L2BlockHeader) -> Self {
        Self {
            number: header.number,
            timestamp: header.timestamp,
            protocol_version: header.protocol_version,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StateKeeperInfo {
    last_sealed_miniblock: Option<L2BlockHeaderInfo>,
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
        let last_sealed_miniblock = conn.blocks_dal().get_last_sealed_l2_block_header().await?;

        self.state_keeper_health_updater.update(
            StateKeeperInfo {
                last_sealed_miniblock: last_sealed_miniblock.map(L2BlockHeaderInfo::from),
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
