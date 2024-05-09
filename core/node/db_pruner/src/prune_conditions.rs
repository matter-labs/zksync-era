use std::{fmt, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use crate::PruneCondition;

#[derive(Debug)]
pub(super) struct L1BatchOlderThanPruneCondition {
    pub minimum_age: Duration,
    pub conn: ConnectionPool<Core>,
}

impl fmt::Display for L1BatchOlderThanPruneCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "L1 Batch is older than {:?}", self.minimum_age)
    }
}

#[async_trait]
impl PruneCondition for L1BatchOlderThanPruneCondition {
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.connection().await?;
        let l1_batch_header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?;
        let is_old_enough = l1_batch_header.is_some()
            && (Utc::now().timestamp() as u64 - l1_batch_header.unwrap().timestamp
                > self.minimum_age.as_secs());
        Ok(is_old_enough)
    }
}

#[derive(Debug)]
pub(super) struct NextL1BatchWasExecutedCondition {
    pub conn: ConnectionPool<Core>,
}

impl fmt::Display for NextL1BatchWasExecutedCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "next L1 batch was executed")
    }
}

#[async_trait]
impl PruneCondition for NextL1BatchWasExecutedCondition {
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.connection().await?;
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

#[derive(Debug)]
pub(super) struct NextL1BatchHasMetadataCondition {
    pub conn: ConnectionPool<Core>,
}

impl fmt::Display for NextL1BatchHasMetadataCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "next L1 batch has metadata")
    }
}

#[async_trait]
impl PruneCondition for NextL1BatchHasMetadataCondition {
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.connection().await?;
        let next_l1_batch_number = L1BatchNumber(l1_batch_number.0 + 1);
        let protocol_version = storage
            .blocks_dal()
            .get_batch_protocol_version_id(next_l1_batch_number)
            .await?;
        // That old l1 batches must have been processed and those old batches are problematic
        // as they have metadata that is not easily retrievable(misses some fields in db)
        let old_protocol_version = protocol_version.map_or(true, |ver| ver.is_pre_1_4_1());
        if old_protocol_version {
            return Ok(true);
        }
        let l1_batch_metadata = storage
            .blocks_dal()
            .get_l1_batch_metadata(next_l1_batch_number)
            .await?;
        Ok(l1_batch_metadata.is_some())
    }
}

#[derive(Debug)]
pub(super) struct L1BatchExistsCondition {
    pub conn: ConnectionPool<Core>,
}

impl fmt::Display for L1BatchExistsCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "L1 batch exists")
    }
}

#[async_trait]
impl PruneCondition for L1BatchExistsCondition {
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.connection().await?;
        let l1_batch_header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?;
        Ok(l1_batch_header.is_some())
    }
}

#[derive(Debug)]
pub(super) struct ConsistencyCheckerProcessedBatch {
    pub conn: ConnectionPool<Core>,
}

impl fmt::Display for ConsistencyCheckerProcessedBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "L1 batch was processed by consistency checker")
    }
}

#[async_trait]
impl PruneCondition for ConsistencyCheckerProcessedBatch {
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.conn.connection().await?;
        let last_processed_l1_batch = storage
            .blocks_dal()
            .get_consistency_checker_last_processed_l1_batch()
            .await?;
        Ok(l1_batch_number <= last_processed_l1_batch)
    }
}
