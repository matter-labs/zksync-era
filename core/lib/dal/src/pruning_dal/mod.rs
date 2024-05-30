use std::ops;

use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L1BatchNumber, L2BlockNumber};

use crate::Core;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct PruningDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

/// Information about Postgres pruning.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PruningInfo {
    pub last_soft_pruned_l1_batch: Option<L1BatchNumber>,
    pub last_soft_pruned_l2_block: Option<L2BlockNumber>,
    pub last_hard_pruned_l1_batch: Option<L1BatchNumber>,
    pub last_hard_pruned_l2_block: Option<L2BlockNumber>,
}

/// Statistics about a single hard pruning iteration.
#[derive(Debug, Default)]
pub struct HardPruningStats {
    pub deleted_l1_batches: u64,
    pub deleted_l2_blocks: u64,
    pub deleted_storage_logs_from_past_batches: u64,
    pub deleted_storage_logs_from_pruned_batches: u64,
    pub deleted_events: u64,
    pub deleted_call_traces: u64,
    pub deleted_l2_to_l1_logs: u64,
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "prune_type")]
enum PruneType {
    Soft,
    Hard,
}

impl PruningDal<'_, '_> {
    pub async fn get_pruning_info(&mut self) -> DalResult<PruningInfo> {
        let pruning_info = sqlx::query!(
            r#"
            WITH
                soft AS (
                    SELECT
                        pruned_l1_batch,
                        pruned_miniblock
                    FROM
                        pruning_log
                    WHERE
                    TYPE = 'Soft'
                    ORDER BY
                        pruned_l1_batch DESC
                    LIMIT
                        1
                ),
                hard AS (
                    SELECT
                        pruned_l1_batch,
                        pruned_miniblock
                    FROM
                        pruning_log
                    WHERE
                    TYPE = 'Hard'
                    ORDER BY
                        pruned_l1_batch DESC
                    LIMIT
                        1
                )
            SELECT
                soft.pruned_l1_batch AS last_soft_pruned_l1_batch,
                soft.pruned_miniblock AS last_soft_pruned_miniblock,
                hard.pruned_l1_batch AS last_hard_pruned_l1_batch,
                hard.pruned_miniblock AS last_hard_pruned_miniblock
            FROM
                soft
                FULL JOIN hard ON TRUE
            "#
        )
        .map(|row| PruningInfo {
            last_soft_pruned_l1_batch: row
                .last_soft_pruned_l1_batch
                .map(|num| L1BatchNumber(num as u32)),
            last_soft_pruned_l2_block: row
                .last_soft_pruned_miniblock
                .map(|num| L2BlockNumber(num as u32)),
            last_hard_pruned_l1_batch: row
                .last_hard_pruned_l1_batch
                .map(|num| L1BatchNumber(num as u32)),
            last_hard_pruned_l2_block: row
                .last_hard_pruned_miniblock
                .map(|num| L2BlockNumber(num as u32)),
        })
        .instrument("get_last_soft_pruned_batch")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;
        Ok(pruning_info.unwrap_or_default())
    }

    pub async fn soft_prune_batches_range(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_l2_block_to_prune: L2BlockNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                pruning_log (
                    pruned_l1_batch,
                    pruned_miniblock,
                    TYPE,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3, NOW(), NOW())
            "#,
            i64::from(last_l1_batch_to_prune.0),
            i64::from(last_l2_block_to_prune.0),
            PruneType::Soft as PruneType,
        )
        .instrument("soft_prune_batches_range#insert_pruning_log")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .with_arg("last_l2_block_to_prune", &last_l2_block_to_prune)
        .with_arg("prune_type", &PruneType::Soft)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn hard_prune_batches_range(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_l2_block_to_prune: L2BlockNumber,
    ) -> DalResult<HardPruningStats> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(number) AS first_miniblock_to_prune
            FROM
                miniblocks
            WHERE
                l1_batch_number <= $1
            "#,
            i64::from(last_l1_batch_to_prune.0),
        )
        .instrument("hard_prune_batches_range#get_miniblocks_range")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        // We don't have any L2 blocks available when recovering from a snapshot
        let stats = if let Some(first_l2_block_to_prune) = row.first_miniblock_to_prune {
            let first_l2_block_to_prune = L2BlockNumber(first_l2_block_to_prune as u32);

            let deleted_events = self
                .delete_events(first_l2_block_to_prune..=last_l2_block_to_prune)
                .await?;
            let deleted_l2_to_l1_logs = self
                .delete_l2_to_l1_logs(first_l2_block_to_prune..=last_l2_block_to_prune)
                .await?;
            let deleted_call_traces = self
                .delete_call_traces(first_l2_block_to_prune..=last_l2_block_to_prune)
                .await?;
            self.clear_transaction_fields(first_l2_block_to_prune..=last_l2_block_to_prune)
                .await?;

            // The deleting of logs is split into two queries to make it faster,
            // only the first query has to go through all previous logs
            // and the query optimizer should be happy with it
            let deleted_storage_logs_from_past_batches = self
                .prune_storage_logs_from_past_l2_blocks(
                    first_l2_block_to_prune..=last_l2_block_to_prune,
                )
                .await?;
            let deleted_storage_logs_from_pruned_batches = self
                .prune_storage_logs_in_range(first_l2_block_to_prune..=last_l2_block_to_prune)
                .await?;
            let deleted_l1_batches = self.delete_l1_batches(last_l1_batch_to_prune).await?;
            let deleted_l2_blocks = self.delete_l2_blocks(last_l2_block_to_prune).await?;

            HardPruningStats {
                deleted_l1_batches,
                deleted_l2_blocks,
                deleted_events,
                deleted_l2_to_l1_logs,
                deleted_call_traces,
                deleted_storage_logs_from_past_batches,
                deleted_storage_logs_from_pruned_batches,
            }
        } else {
            HardPruningStats::default()
        };

        self.insert_hard_pruning_log(last_l1_batch_to_prune, last_l2_block_to_prune)
            .await?;
        Ok(stats)
    }

    async fn delete_events(
        &mut self,
        l2_blocks_to_prune: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM events
            WHERE
                miniblock_number BETWEEN $1 AND $2
            "#,
            i64::from(l2_blocks_to_prune.start().0),
            i64::from(l2_blocks_to_prune.end().0)
        )
        .instrument("hard_prune_batches_range#delete_events")
        .with_arg("l2_blocks_to_prune", &l2_blocks_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    async fn delete_l2_to_l1_logs(
        &mut self,
        l2_blocks_to_prune: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM l2_to_l1_logs
            WHERE
                miniblock_number BETWEEN $1 AND $2
            "#,
            i64::from(l2_blocks_to_prune.start().0),
            i64::from(l2_blocks_to_prune.end().0)
        )
        .instrument("hard_prune_batches_range#delete_l2_to_l1_logs")
        .with_arg("l2_blocks_to_prune", &l2_blocks_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    // Call traces are returned via `TransactionsDal::get_call_trace()`, which is used by the `debug_traceTransaction` RPC method.
    // It should be acceptable to return `None` for transactions in pruned L2 blocks; this would make them indistinguishable
    // from traces for non-existing transactions.
    async fn delete_call_traces(
        &mut self,
        l2_blocks_to_prune: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM call_traces
            WHERE
                tx_hash IN (
                    SELECT
                        hash
                    FROM
                        transactions
                    WHERE
                        miniblock_number BETWEEN $1 AND $2
                )
            "#,
            i64::from(l2_blocks_to_prune.start().0),
            i64::from(l2_blocks_to_prune.end().0)
        )
        .instrument("hard_prune_batches_range#delete_call_traces")
        .with_arg("l2_blocks_to_prune", &l2_blocks_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    // The pruned fields are accessed as follows:
    //
    // - `input`: is a part of `StorageTransaction`, read via `TransactionsDal` (`get_l2_blocks_to_reexecute`,
    //   `get_l2_blocks_to_execute_for_l1_batch`, and `get_tx_by_hash`) and `TransactionsWeb3Dal::get_raw_l2_block_transactions()`.
    //   `get_tx_by_hash()` is only called on upgrade transactions, which are not pruned. The remaining methods tie transactions
    //   to a certain L1 batch / L2 block, and thus do naturally check pruning.
    // - `data`: used by `TransactionsWeb3Dal` queries, which explicitly check whether it was pruned.
    // - `execution_info`: not used in queries.
    async fn clear_transaction_fields(
        &mut self,
        l2_blocks_to_prune: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            UPDATE transactions
            SET
                input = NULL,
                data = '{}',
                execution_info = '{}',
                updated_at = NOW()
            WHERE
                miniblock_number BETWEEN $1 AND $2
                AND upgrade_id IS NULL
            "#,
            i64::from(l2_blocks_to_prune.start().0),
            i64::from(l2_blocks_to_prune.end().0)
        )
        .instrument("hard_prune_batches_range#clear_transaction_fields")
        .with_arg("l2_blocks_to_prune", &l2_blocks_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    async fn prune_storage_logs_from_past_l2_blocks(
        &mut self,
        l2_blocks_to_prune: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM storage_logs
            WHERE
                storage_logs.miniblock_number < $1
                AND hashed_key IN (
                    SELECT
                        hashed_key
                    FROM
                        storage_logs
                    WHERE
                        miniblock_number BETWEEN $1 AND $2
                )
            "#,
            i64::from(l2_blocks_to_prune.start().0),
            i64::from(l2_blocks_to_prune.end().0)
        )
        .instrument("hard_prune_batches_range#prune_storage_logs_from_past_l2_blocks")
        .with_arg("l2_blocks_to_prune", &l2_blocks_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    async fn prune_storage_logs_in_range(
        &mut self,
        l2_blocks_to_prune: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM storage_logs USING (
                SELECT
                    hashed_key,
                    MAX(ARRAY[miniblock_number, operation_number]::INT[]) AS op
                FROM
                    storage_logs
                WHERE
                    miniblock_number BETWEEN $1 AND $2
                GROUP BY
                    hashed_key
            ) AS last_storage_logs
            WHERE
                storage_logs.miniblock_number BETWEEN $1 AND $2
                AND last_storage_logs.hashed_key = storage_logs.hashed_key
                AND (
                    storage_logs.miniblock_number != last_storage_logs.op[1]
                    OR storage_logs.operation_number != last_storage_logs.op[2]
                )
            "#,
            i64::from(l2_blocks_to_prune.start().0),
            i64::from(l2_blocks_to_prune.end().0)
        )
        .instrument("hard_prune_batches_range#prune_storage_logs_in_range")
        .with_arg("l2_blocks_to_prune", &l2_blocks_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    async fn delete_l1_batches(&mut self, last_l1_batch_to_prune: L1BatchNumber) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM l1_batches
            WHERE
                number <= $1
            "#,
            i64::from(last_l1_batch_to_prune.0),
        )
        .instrument("hard_prune_batches_range#delete_l1_batches")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    async fn delete_l2_blocks(&mut self, last_l2_block_to_prune: L2BlockNumber) -> DalResult<u64> {
        let execution_result = sqlx::query!(
            r#"
            DELETE FROM miniblocks
            WHERE
                number <= $1
            "#,
            i64::from(last_l2_block_to_prune.0),
        )
        .instrument("hard_prune_batches_range#delete_l2_blocks")
        .with_arg("last_l2_block_to_prune", &last_l2_block_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(execution_result.rows_affected())
    }

    async fn insert_hard_pruning_log(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_l2_block_to_prune: L2BlockNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                pruning_log (
                    pruned_l1_batch,
                    pruned_miniblock,
                    TYPE,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3, NOW(), NOW())
            "#,
            i64::from(last_l1_batch_to_prune.0),
            i64::from(last_l2_block_to_prune.0),
            PruneType::Hard as PruneType
        )
        .instrument("hard_prune_batches_range#insert_pruning_log")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .with_arg("last_l2_block_to_prune", &last_l2_block_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }
}
