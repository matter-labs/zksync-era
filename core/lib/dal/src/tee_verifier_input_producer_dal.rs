use std::time::{Duration, Instant};

use sqlx::postgres::types::PgInterval;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::InstrumentExt,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};
use zksync_types::L1BatchNumber;

use crate::Core;

#[derive(Debug)]
pub struct TeeVerifierInputProducerDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

/// The amount of attempts to process a job before giving up.
pub const JOB_MAX_ATTEMPT: i16 = 2;

/// Time to wait for job to be processed
const JOB_PROCESSING_TIMEOUT: PgInterval = pg_interval_from_duration(Duration::from_secs(10 * 60));

/// Status of a job that the producer will work on.

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "tee_verifier_input_producer_job_status")]
pub enum TeeVerifierInputProducerJobStatus {
    /// When the job is queued. Metadata calculator creates the job and marks it as queued.
    Queued,
    /// The job is not going to be processed. This state is designed for manual operations on DB.
    /// It is expected to be used if some jobs should be skipped like:
    /// - testing purposes (want to check a specific L1 Batch, I can mark everything before it skipped)
    /// - trim down costs on some environments (if I've done breaking changes,
    ///   makes no sense to wait for everything to be processed, I can just skip them and save resources)
    ManuallySkipped,
    /// Currently being processed by one of the jobs. Transitory state, will transition to either
    /// [`TeeVerifierInputProducerStatus::Successful`] or [`TeeVerifierInputProducerStatus::Failed`].
    InProgress,
    /// The final (happy case) state we expect all jobs to end up. After the run is complete,
    /// the job uploaded it's inputs, it lands in successful.
    Successful,
    /// The job failed for reasons. It will be marked as such and the error persisted in DB.
    /// If it failed less than MAX_ATTEMPTs, the job will be retried,
    /// otherwise it will stay in this state as final state.
    Failed,
}

impl TeeVerifierInputProducerDal<'_, '_> {
    pub async fn create_tee_verifier_input_producer_job(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                tee_verifier_input_producer_jobs (l1_batch_number, status, created_at, updated_at)
            VALUES
                ($1, $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(l1_batch_number.0),
            TeeVerifierInputProducerJobStatus::Queued as TeeVerifierInputProducerJobStatus,
        )
        .instrument("create_tee_verifier_input_producer_job")
        .with_arg("l1_batch_number", &l1_batch_number)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_new_l1_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        // Since we depend on Merkle paths, we use the proof_generation_details table to inform us of newly available to-be-proven batches.
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(l1_batch_number) AS "number"
            FROM
                proof_generation_details
            WHERE
                proof_gen_data_blob_url IS NOT NULL
            "#
        )
        .instrument("get_sealed_l1_batch_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?
        .number;

        let max_sealed = match row {
            Some(number) => number,
            None => return Ok(None),
        };

        let row = sqlx::query!(
            r#"
            SELECT
                MAX(l1_batch_number) AS "number"
            FROM
                tee_verifier_input_producer_jobs
            "#
        )
        .instrument("get_latest_tee_verifier_input_producer_jobs")
        .report_latency()
        .fetch_one(self.storage)
        .await?
        .number;

        match row {
            // If no batches have been processed by TEE so far, i.e., table is empty, we start with the most recent L1 batch.
            None => Ok(Some(L1BatchNumber(max_sealed as u32))),
            Some(max_tee_batch_number) => {
                if max_sealed > max_tee_batch_number {
                    Ok(Some(L1BatchNumber(max_tee_batch_number as u32 + 1)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub async fn get_next_tee_verifier_input_producer_job(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
        if let Some(n) = self.get_new_l1_batch().await? {
            self.create_tee_verifier_input_producer_job(n).await?;
        }

        let l1_batch_number = sqlx::query!(
            r#"
            UPDATE tee_verifier_input_producer_jobs
            SET
                status = $1,
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        tee_verifier_input_producer_jobs
                    WHERE
                        status = $2
                        OR (
                            status = $1
                            AND processing_started_at < NOW() - $4::INTERVAL
                        )
                        OR (
                            status = $3
                            AND attempts < $5
                        )
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                tee_verifier_input_producer_jobs.l1_batch_number
            "#,
            TeeVerifierInputProducerJobStatus::InProgress as TeeVerifierInputProducerJobStatus,
            TeeVerifierInputProducerJobStatus::Queued as TeeVerifierInputProducerJobStatus,
            TeeVerifierInputProducerJobStatus::Failed as TeeVerifierInputProducerJobStatus,
            &JOB_PROCESSING_TIMEOUT,
            JOB_MAX_ATTEMPT,
        )
        .instrument("get_next_tee_verifier_input_producer_job")
        .report_latency()
        .fetch_optional(self.storage)
        .await?
        .map(|job| L1BatchNumber(job.l1_batch_number as u32));

        Ok(l1_batch_number)
    }

    pub async fn get_tee_verifier_input_producer_job_attempts(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                tee_verifier_input_producer_jobs
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("get_tee_verifier_input_producer_job_attempts")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .map(|job| job.attempts as u32);

        Ok(attempts)
    }

    pub async fn mark_job_as_successful(
        &mut self,
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
        object_path: &str,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_verifier_input_producer_jobs
            SET
                status = $1,
                updated_at = NOW(),
                time_taken = $3,
                input_blob_url = $4
            WHERE
                l1_batch_number = $2
            "#,
            TeeVerifierInputProducerJobStatus::Successful as TeeVerifierInputProducerJobStatus,
            i64::from(l1_batch_number.0),
            duration_to_naive_time(started_at.elapsed()),
            object_path,
        )
        .instrument("mark_job_as_successful")
        .with_arg("l1_batch_number", &l1_batch_number)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_job_as_failed(
        &mut self,
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
        error: String,
    ) -> DalResult<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            UPDATE tee_verifier_input_producer_jobs
            SET
                status = $1,
                updated_at = NOW(),
                time_taken = $3,
                error = $4
            WHERE
                l1_batch_number = $2
                AND status != $5
            RETURNING
                tee_verifier_input_producer_jobs.attempts
            "#,
            TeeVerifierInputProducerJobStatus::Failed as TeeVerifierInputProducerJobStatus,
            i64::from(l1_batch_number.0),
            duration_to_naive_time(started_at.elapsed()),
            error,
            TeeVerifierInputProducerJobStatus::Successful as TeeVerifierInputProducerJobStatus,
        )
        .instrument("mark_job_as_failed")
        .with_arg("l1_batch_number", &l1_batch_number)
        .report_latency()
        .fetch_optional(self.storage)
        .await?
        .map(|job| job.attempts as u32);

        Ok(attempts)
    }
}

/// These functions should only be used for tests.
impl TeeVerifierInputProducerDal<'_, '_> {
    pub async fn delete_all_jobs(&mut self) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM tee_verifier_input_producer_jobs
            "#
        )
        .instrument("delete_all_tee_verifier_jobs")
        .execute(self.storage)
        .await?;
        Ok(())
    }
}
