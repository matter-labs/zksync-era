use std::cmp::max;

use anyhow::Ok;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater};
use zksync_types::{aggregated_operations::AggregatedActionType, L1BatchNumber};

use crate::periodic_job::PeriodicJob;

#[derive(Debug, Serialize, Deserialize)]
struct BatchNumbers {
    commit: Option<L1BatchNumber>,
    prove: Option<L1BatchNumber>,
    execute: Option<L1BatchNumber>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EthSenderInfo {
    failed_l1_txns: i64,
    last_saved_batches: BatchNumbers,
    last_mined_batches: BatchNumbers,
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
    async fn run(&mut self) -> anyhow::Result<()> {
        self.run_routine_task().await
    }

    async fn run_routine_task(&mut self) -> anyhow::Result<()> {
        let mut conn = self.connection_pool.connection().await?;
        let failed_l1_txns = conn
            .eth_sender_dal()
            .get_number_of_failed_transactions()
            .await?;

        let eth_stats = conn.eth_sender_dal().get_eth_l1_batches().await?;

        // TODO retrieve SettlementMode from config
        let next_nonce = conn.eth_sender_dal().get_next_nonce(None, false).await?;

        self.eth_sender_health_updater.update(
            EthSenderInfo {
                failed_l1_txns,
                last_saved_batches: get_latest_batches(eth_stats.saved),
                last_mined_batches: get_latest_batches(eth_stats.mined),
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

fn get_latest_batches(batches: Vec<(AggregatedActionType, L1BatchNumber)>) -> BatchNumbers {
    let (commit_batch, prove_batch, execute_batch) = batches.into_iter().fold(
        (None, None, None),
        |(commit, prove, execute), (action_type, batch_number)| match action_type {
            AggregatedActionType::Commit => (max(commit, Some(batch_number)), prove, execute),
            AggregatedActionType::PublishProofOnchain => {
                (commit, max(prove, Some(batch_number)), execute)
            }
            AggregatedActionType::Execute => (commit, prove, max(execute, Some(batch_number))),
        },
    );

    BatchNumbers {
        commit: commit_batch,
        prove: prove_batch,
        execute: execute_batch,
    }
}
