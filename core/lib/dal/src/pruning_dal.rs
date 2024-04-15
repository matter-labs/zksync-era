use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L1BatchNumber, MiniblockNumber};

use crate::Core;

#[derive(Debug)]
pub struct PruningDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PruningInfo {
    pub last_soft_pruned_l1_batch: Option<L1BatchNumber>,
    pub last_soft_pruned_miniblock: Option<MiniblockNumber>,
    pub last_hard_pruned_l1_batch: Option<L1BatchNumber>,
    pub last_hard_pruned_miniblock: Option<MiniblockNumber>,
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "prune_type")]
pub enum PruneType {
    Soft,
    Hard,
}

impl PruningDal<'_, '_> {
    pub async fn get_pruning_info(&mut self) -> DalResult<PruningInfo> {
        let row = sqlx::query!(
            r#"
            SELECT
                soft.pruned_l1_batch AS last_soft_pruned_l1_batch,
                soft.pruned_miniblock AS last_soft_pruned_miniblock,
                hard.pruned_l1_batch AS last_hard_pruned_l1_batch,
                hard.pruned_miniblock AS last_hard_pruned_miniblock
            FROM
                (
                    SELECT
                        1
                ) AS dummy
                LEFT JOIN (
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
                ) AS soft ON TRUE
                LEFT JOIN (
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
                ) AS hard ON TRUE;
            "#
        )
        .instrument("get_last_soft_pruned_batch")
        .report_latency()
        .fetch_one(self.storage)
        .await?;
        Ok(PruningInfo {
            last_soft_pruned_l1_batch: row
                .last_soft_pruned_l1_batch
                .map(|x| L1BatchNumber(x as u32)),
            last_soft_pruned_miniblock: row
                .last_soft_pruned_miniblock
                .map(|x| MiniblockNumber(x as u32)),
            last_hard_pruned_l1_batch: row
                .last_hard_pruned_l1_batch
                .map(|x| L1BatchNumber(x as u32)),
            last_hard_pruned_miniblock: row
                .last_hard_pruned_miniblock
                .map(|x| MiniblockNumber(x as u32)),
        })
    }

    pub async fn soft_prune_batches_range(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_miniblock_to_prune: MiniblockNumber,
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
            i64::from(last_miniblock_to_prune.0),
            PruneType::Soft as PruneType,
        )
        .instrument("soft_prune_batches_range#insert_pruning_log")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .with_arg("prune_type", &PruneType::Soft)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn hard_prune_batches_range(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_miniblock_to_prune: MiniblockNumber,
    ) -> DalResult<()> {
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

        // we don't have any miniblocks available when recovering from a snapshot
        if row.first_miniblock_to_prune.is_some() {
            let first_miniblock_to_prune =
                MiniblockNumber(row.first_miniblock_to_prune.unwrap() as u32);

            let deleted_events = sqlx::query!(
                r#"
                WITH
                    deleted AS (
                        DELETE FROM events
                        WHERE
                            miniblock_number <= $1
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument("hard_prune_batches_range#delete_events")
            .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            let deleted_l2_to_l1_logs = sqlx::query!(
                r#"
                WITH
                    deleted AS (
                        DELETE FROM l2_to_l1_logs
                        WHERE
                            miniblock_number <= $1
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument("hard_prune_batches_range#delete_l2_to_l1_logs")
            .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            let deleted_call_traces = sqlx::query!(
                r#"
                WITH
                    deleted AS (
                        DELETE FROM call_traces USING (
                            SELECT
                                *
                            FROM
                                transactions
                            WHERE
                                miniblock_number BETWEEN $1 AND $2
                        ) AS matching_transactions
                        WHERE
                            matching_transactions.hash = call_traces.tx_hash
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(first_miniblock_to_prune.0),
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument("hard_prune_batches_range#delete_call_traces")
            .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
            .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            sqlx::query!(
                r#"
                WITH
                    updated AS (
                        UPDATE transactions
                        SET
                            input = NULL,
                            data = '{}',
                            execution_info = '{}',
                            updated_at = NOW()
                        WHERE
                            miniblock_number BETWEEN $1 AND $2
                            AND upgrade_id IS NULL
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    updated
                "#,
                i64::from(first_miniblock_to_prune.0),
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument("hard_prune_batches_range#clear_transactions_references")
            .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
            .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            //The deleting of logs is split into two queries to make it faster,
            // only the first query has to go through all previous logs
            // and the query optimizer should be happy with it
            let deleted_storage_logs_from_past_batches = sqlx::query!(
                r#"
                WITH
                    deleted AS (
                        DELETE FROM storage_logs USING (
                            SELECT
                                *
                            FROM
                                storage_logs
                            WHERE
                                miniblock_number BETWEEN $1 AND $2
                        ) AS batches_to_prune
                        WHERE
                            storage_logs.miniblock_number < $1
                            AND batches_to_prune.hashed_key = storage_logs.hashed_key
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(first_miniblock_to_prune.0),
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument("hard_prune_batches_range#delete_overriden_storage_logs_from_past_batches")
            .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
            .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            let deleted_storage_logs_from_pruned_batches = sqlx::query!(
                r#"
                WITH
                    deleted AS (
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
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(first_miniblock_to_prune.0),
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument(
                "hard_prune_batches_range#delete_overriden_storage_logs_from_pruned_batches",
            )
            .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
            .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            let deleted_l1_batches = sqlx::query!(
                r#"
                WITH
                    deleted AS (
                        DELETE FROM l1_batches
                        WHERE
                            number <= $1
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(last_l1_batch_to_prune.0),
            )
            .instrument("hard_prune_batches_range#delete_l1_batches")
            .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            let deleted_miniblocks = sqlx::query!(
                r#"
                WITH
                    deleted AS (
                        DELETE FROM miniblocks
                        WHERE
                            number <= $1
                        RETURNING
                            *
                    )
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    deleted
                "#,
                i64::from(last_miniblock_to_prune.0),
            )
            .instrument("hard_prune_batches_range#delete_miniblocks")
            .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
            .report_latency()
            .fetch_one(self.storage)
            .await?;

            tracing::info!("Performed pruning of database, deleted {} l1_batches, {} miniblocks, {} storage_logs, {} events, {} call traces, {} l2_to_l1_logs",
                deleted_l1_batches.count,
                deleted_miniblocks.count,
                deleted_storage_logs_from_past_batches.count + deleted_storage_logs_from_pruned_batches.count,
                deleted_events.count,
                deleted_call_traces.count,
                deleted_l2_to_l1_logs.count)
        }

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
            i64::from(last_miniblock_to_prune.0),
            PruneType::Hard as PruneType
        )
        .instrument("hard_prune_batches_range#insert_pruning_log")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .with_arg("prune_type", &PruneType::Hard)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    // This method must be separate as VACUUM is not supported inside a transaction
    pub async fn run_vacuum_after_hard_pruning(&mut self) -> DalResult<()> {
        sqlx::query!(
            r#"
            VACUUM l1_batches,
            miniblocks,
            storage_logs,
            events,
            call_traces,
            l2_to_l1_logs,
            transactions
            "#,
        )
        .instrument("hard_prune_batches_range#vacuum")
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }
}
