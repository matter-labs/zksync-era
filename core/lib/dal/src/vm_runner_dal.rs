use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::L1BatchNumber;

use crate::Core;

#[derive(Debug)]
pub struct VmRunnerDal<'c, 'a> {
    pub(crate) storage: &'c mut Connection<'a, Core>,
}

impl VmRunnerDal<'_, '_> {
    pub async fn get_protective_reads_latest_processed_batch(
        &mut self,
        default_batch: L1BatchNumber,
    ) -> DalResult<L1BatchNumber> {
        let row = sqlx::query!(
            r#"
            SELECT
                COALESCE(MAX(l1_batch_number), $1) AS "last_processed_l1_batch!"
            FROM
                vm_runner_protective_reads
            WHERE
                time_taken IS NOT NULL
            "#,
            default_batch.0 as i32
        )
        .instrument("get_protective_reads_latest_processed_batch")
        .report_latency()
        .fetch_one(self.storage)
        .await?;
        Ok(L1BatchNumber(row.last_processed_l1_batch as u32))
    }

    pub async fn get_protective_reads_last_ready_batch(
        &mut self,
        default_batch: L1BatchNumber,
        window_size: u32,
    ) -> DalResult<L1BatchNumber> {
        let row = sqlx::query!(
            r#"
            WITH
                available_batches AS (
                    SELECT
                        MAX(number) AS "last_batch"
                    FROM
                        l1_batches
                ),
                processed_batches AS (
                    SELECT
                        COALESCE(MAX(l1_batch_number), $1) + $2 AS "last_ready_batch"
                    FROM
                        vm_runner_protective_reads
                    WHERE
                        time_taken IS NOT NULL
                )
            SELECT
                LEAST(last_batch, last_ready_batch) AS "last_ready_batch!"
            FROM
                available_batches
                FULL JOIN processed_batches ON TRUE
            "#,
            default_batch.0 as i32,
            window_size as i32
        )
        .instrument("get_protective_reads_last_ready_batch")
        .report_latency()
        .fetch_one(self.storage)
        .await?;
        Ok(L1BatchNumber(row.last_ready_batch as u32))
    }

    pub async fn mark_protective_reads_batch_as_processing(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                vm_runner_protective_reads (l1_batch_number, created_at, updated_at, processing_started_at)
            VALUES
                ($1, NOW(), NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
                updated_at = NOW(),
                processing_started_at = NOW()
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("mark_protective_reads_batch_as_processing")
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn mark_protective_reads_batch_as_completed(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let update_result = sqlx::query!(
            r#"
            UPDATE vm_runner_protective_reads
            SET
                time_taken = NOW() - processing_started_at
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("mark_protective_reads_batch_as_completed")
        .report_latency()
        .execute(self.storage)
        .await?;
        if update_result.rows_affected() == 0 {
            anyhow::bail!(
                "Trying to mark an L1 batch as completed while it is not being processed"
            );
        }
        Ok(())
    }

    pub async fn delete_protective_reads(
        &mut self,
        last_batch_to_keep: L1BatchNumber,
    ) -> DalResult<()> {
        self.delete_protective_reads_inner(Some(last_batch_to_keep))
            .await
    }

    async fn delete_protective_reads_inner(
        &mut self,
        last_batch_to_keep: Option<L1BatchNumber>,
    ) -> DalResult<()> {
        let l1_batch_number = last_batch_to_keep.map_or(-1, |number| i64::from(number.0));
        sqlx::query!(
            r#"
            DELETE FROM vm_runner_protective_reads
            WHERE
                l1_batch_number > $1
            "#,
            l1_batch_number
        )
        .instrument("delete_protective_reads")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}
