use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};

use crate::periodic_job::PeriodicJob;

#[derive(Debug, Serialize, Deserialize)]
pub struct EthSenderInfo {
    failed_l1_txns: i64,
    last_created_commit_batch: Option<()>,
    last_created_prove_batch: Option<()>,
    last_created_execute_batch: Option<()>,
    last_executed_commit_batch: Option<()>,
    last_executed_prove_batch: Option<()>,
    last_executed_execute_batch: Option<()>,
    next_nonce: Option<u64>,
}

impl From<EthSenderInfo> for Health {
    fn from(details: EthSenderInfo) -> Self {
        Self::from(HealthStatus::Ready).with_details(details)
    }
}

#[derive(Debug)]
pub struct EthSenderHealthTask {
    pub connection_pool: ConnectionPool<Core>,
    pub eth_sender_health_updater: HealthUpdater,
}

impl EthSenderHealthTask {
    pub const POLLING_INTERVAL_MS: u64 = 10_000;
}

#[async_trait]
impl PeriodicJob for EthSenderHealthTask {
    const SERVICE_NAME: &'static str = "EthSenderHealth";

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await?;
        let failed_l1_txns = conn
            .eth_sender_dal()
            .get_number_of_failed_transactions()
            .await?;

        // TODO retrieve SettlementMode from config
        let next_nonce = conn.eth_sender_dal().get_next_nonce(None, false).await?;

        self.eth_sender_health_updater.update(
            EthSenderInfo {
                failed_l1_txns,
                last_created_commit_batch: None,
                last_created_prove_batch: None,
                last_created_execute_batch: None,
                last_executed_commit_batch: None,
                last_executed_prove_batch: None,
                last_executed_execute_batch: None,
                next_nonce,
            }
            .into(),
        );
        Ok(())
    }

    fn polling_interval_ms(&self) -> u64 {
        Self::POLLING_INTERVAL_MS
    }
}
