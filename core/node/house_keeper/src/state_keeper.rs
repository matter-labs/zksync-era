use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};
use zksync_types::{block::L2BlockHeader, L1BatchNumber, L2BlockNumber, ProtocolVersionId};

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
    last_processed_l1_batch: L1BatchNumber,
}

impl From<StateKeeperInfo> for Health {
    fn from(details: StateKeeperInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug)]
pub struct StateKeeperHealthTask {
    pub polling_interval_ms: u64,
    pub connection_pool: ConnectionPool<Core>,
    pub state_keeper_health_updater: HealthUpdater,
}

#[async_trait]
impl PeriodicJob for StateKeeperHealthTask {
    const SERVICE_NAME: &'static str = "StateKeeperHealth";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await.unwrap();
        let last_sealed_miniblock = conn.blocks_dal().get_last_sealed_l2_block_header().await?;
        let last_processed_l1_batch = conn
            .blocks_dal()
            .get_consistency_checker_last_processed_l1_batch()
            .await?;

        self.state_keeper_health_updater.update(
            StateKeeperInfo {
                last_sealed_miniblock: last_sealed_miniblock.map(L2BlockHeaderInfo::from),
                last_processed_l1_batch,
            }
            .into(),
        );
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        self.polling_interval_ms
    }
}
