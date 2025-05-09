use std::{fmt, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

#[async_trait]
pub(crate) trait PruneCondition: fmt::Debug + fmt::Display + Send + Sync + 'static {
    fn metric_label(&self) -> &'static str;

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool>;
}

#[derive(Debug)]
pub(super) struct L1BatchOlderThanPruneCondition {
    pub minimum_age: Duration,
    pub pool: ConnectionPool<Core>,
}

impl fmt::Display for L1BatchOlderThanPruneCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "L1 Batch is older than {:?}", self.minimum_age)
    }
}

#[async_trait]
impl PruneCondition for L1BatchOlderThanPruneCondition {
    fn metric_label(&self) -> &'static str {
        "l1_batch_older_than_minimum_age"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("db_pruner").await?;
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
    pub pool: ConnectionPool<Core>,
}

impl fmt::Display for NextL1BatchWasExecutedCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("next L1 batch was executed")
    }
}

#[async_trait]
impl PruneCondition for NextL1BatchWasExecutedCondition {
    fn metric_label(&self) -> &'static str {
        "next_l1_batch_was_executed"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("db_pruner").await?;
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
    pub pool: ConnectionPool<Core>,
}

impl fmt::Display for NextL1BatchHasMetadataCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("next L1 batch has metadata")
    }
}

#[async_trait]
impl PruneCondition for NextL1BatchHasMetadataCondition {
    fn metric_label(&self) -> &'static str {
        "next_l1_batch_has_metadata"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("db_pruner").await?;
        let next_l1_batch_number = L1BatchNumber(l1_batch_number.0 + 1);
        let Some(batch) = storage
            .blocks_dal()
            .get_optional_l1_batch_metadata(next_l1_batch_number)
            .await?
        else {
            return Ok(false);
        };

        Ok(if let Err(err) = &batch.metadata {
            // Metadata may be incomplete for very old batches on full nodes.
            let protocol_version = batch.header.protocol_version;
            let is_old = protocol_version.is_none_or(|ver| ver.is_pre_1_4_1());
            if is_old {
                tracing::info!(
                    "Error getting metadata for L1 batch #{next_l1_batch_number} \
                    with old protocol version {protocol_version:?}: {err}"
                );
            }
            is_old
        } else {
            true
        })
    }
}

#[derive(Debug)]
pub(super) struct L1BatchExistsCondition {
    pub pool: ConnectionPool<Core>,
}

impl fmt::Display for L1BatchExistsCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("L1 batch exists")
    }
}

#[async_trait]
impl PruneCondition for L1BatchExistsCondition {
    fn metric_label(&self) -> &'static str {
        "l1_batch_exists"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("db_pruner").await?;
        let l1_batch_header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?;
        Ok(l1_batch_header.is_some())
    }
}

#[derive(Debug)]
pub(super) struct ConsistencyCheckerProcessedBatch {
    pub pool: ConnectionPool<Core>,
}

impl fmt::Display for ConsistencyCheckerProcessedBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("L1 batch was processed by consistency checker")
    }
}

#[async_trait]
impl PruneCondition for ConsistencyCheckerProcessedBatch {
    fn metric_label(&self) -> &'static str {
        "l1_batch_consistency_checked"
    }

    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("db_pruner").await?;
        let last_processed_l1_batch = storage
            .blocks_dal()
            .get_consistency_checker_last_processed_l1_batch()
            .await?;
        Ok(l1_batch_number <= last_processed_l1_batch)
    }
}
