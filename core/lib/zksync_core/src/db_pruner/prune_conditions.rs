use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use zksync_dal::ConnectionPool;
use zksync_types::L1BatchNumber;

use crate::db_pruner::PruneCondition;

pub struct L1BatchOlderThanPruneCondition {
    pub minimal_age: Duration,
    pub conn: ConnectionPool,
}

#[async_trait]
impl PruneCondition for L1BatchOlderThanPruneCondition {
    fn name(&self) -> &'static str {
        "l1 Batch is old enough"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.access_storage().await?;
        let l1_batch_header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?;
        let is_old_enough = l1_batch_header.is_some()
            && (Utc::now().timestamp() as u64 - l1_batch_header.unwrap().timestamp
                > self.minimal_age.as_secs());
        Ok(is_old_enough)
    }
}

pub struct NextL1BatchWasExecutedCondition {
    pub conn: ConnectionPool,
}

#[async_trait]
impl PruneCondition for NextL1BatchWasExecutedCondition {
    fn name(&self) -> &'static str {
        "next l1 batch was executed"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.access_storage().await?;
        let next_l1_batch_number = L1BatchNumber(l1_batch_number.0 + 1);
        let last_executed_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_executed_on_eth()
            .await?;
        let was_next_batch_executed =
            last_executed_batch.is_some() && last_executed_batch.unwrap() >= next_l1_batch_number;
        Ok(was_next_batch_executed)
    }
}

pub struct NextL1BatchHasMetadataCondition {
    pub conn: ConnectionPool,
}

#[async_trait]
impl PruneCondition for NextL1BatchHasMetadataCondition {
    fn name(&self) -> &'static str {
        "next l1 batch has metadata"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.access_storage().await?;
        let next_l1_batch_number = L1BatchNumber(l1_batch_number.0 + 1);
        let l1_batch_metadata = storage
            .blocks_dal()
            .get_l1_batch_metadata(next_l1_batch_number)
            .await?;
        Ok(l1_batch_metadata.is_some())
    }
}

pub struct L1BatchExistsCondition {
    pub conn: ConnectionPool,
}

#[async_trait]
impl PruneCondition for L1BatchExistsCondition {
    fn name(&self) -> &'static str {
        "l1 batch exists"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.access_storage().await?;
        let l1_batch_header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?;
        Ok(l1_batch_header.is_some())
    }
}
