use crate::instrument::InstrumentExt;
use crate::time_utils::{duration_to_naive_time, pg_interval_from_duration};
use crate::StorageProcessor;
use std::time::{Duration, Instant};
use zksync_types::L1BatchNumber;

#[derive(Debug)]
pub struct BasicWitnessInputProducerDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

/// Status of a job that the producer will work on.
#[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
pub enum BasicWitnessInputProducerStatus {
    /// When the job is queued. It can end in this state either from mempool at creation time or
    /// from house keeper, in case the job's been pending for too long (in_progress > MAX_TIME) or
    /// if it failed no more than MAX_ATTEMPT times.
    #[strum(serialize = "queued")]
    Queued,
    /// The job is not going to be processed. This state is designed for manual operations on DB.
    /// It is expected to be used if some jobs should be skipped like:
    /// - testing purposes (want to check a specific L1 Batch, I can mark everything before it skipped)
    /// - trim down costs on some environments (if I've done breaking changes,
    /// makes no sense to wait for everything to be processed, I can just skip them and save resources)
    #[strum(serialize = "manually_skipped")]
    ManuallySkipped,
    /// Currently being processed by one of the jobs. Transitory state, will transition to either
    /// [`BasicWitnessInputProducerStatus::Successful`] or [`BasicWitnessInputProducerStatus::Failed`].
    #[strum(serialize = "in_progress")]
    InProgress,
    /// The final (happy case) state we expect all jobs to end up. After the run is complete,
    /// the job uploaded it's inputs, it lands in successful.
    #[strum(serialize = "successful")]
    Successful,
    /// The job failed for reasons. It will be marked as such and the error persisted in DB.
    /// If it failed less than MAX_ATTEMPTs, house_keeper will move it back to [`BasicWitnessInputProducerStatus::Queued`],
    /// otherwise it will stay in this state as final state.
    #[strum(serialize = "failed")]
    Failed,
}

impl BasicWitnessInputProducerDal<'_, '_> {
    pub async fn create_basic_witness_input_producer_job(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) {
        sqlx::query!(
            "INSERT INTO basic_witness_input_producer_jobs
                (l1_batch_number, status, created_at, updated_at)
            VALUES ($1, $2, now(), now())",
            l1_batch_number.0 as i64,
            format!("{}", BasicWitnessInputProducerStatus::Queued),
        )
        .instrument("create_basic_witness_input_producer_job")
        .report_latency()
        .execute(self.storage.conn())
        .await
        .expect("failed to create basic witness input producer job");
    }

    pub async fn get_next_basic_witness_input_producer_job(&mut self) -> Option<L1BatchNumber> {
        let max_attempts = 5;
        let processing_timeout = pg_interval_from_duration(Duration::from_secs(60));
        sqlx::query!(
            "UPDATE basic_witness_input_producer_jobs
            SET status = $1,
                attempts = attempts + 1,
                updated_at = now(),
                processing_started_at = now()
            WHERE l1_batch_number = (
                SELECT MIN(l1_batch_number)
                FROM basic_witness_input_producer_jobs
                WHERE status = $2 OR
                    (status = $1 AND processing_started_at < now() - $4::interval) OR
                    (status = $3 AND attempts < $5)
            )
            RETURNING basic_witness_input_producer_jobs.l1_batch_number",
            format!("{}", BasicWitnessInputProducerStatus::InProgress),
            format!("{}", BasicWitnessInputProducerStatus::Queued),
            format!("{}", BasicWitnessInputProducerStatus::Failed),
            &processing_timeout,
            max_attempts as i32,
        )
        .instrument("get_next_basic_witness_input_producer_job")
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await
        .expect("failed to get next basic witness input producer job")
        .map(|job| L1BatchNumber(job.l1_batch_number as u32))
    }

    pub async fn mark_job_as_successful(
        &mut self,
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
    ) {
        sqlx::query!(
            "UPDATE basic_witness_input_producer_jobs
            SET status = $1, updated_at = now(), time_taken = $3
            WHERE l1_batch_number = $2",
            format!("{}", BasicWitnessInputProducerStatus::Successful),
            l1_batch_number.0 as i64,
            duration_to_naive_time(started_at.elapsed()),
        )
        .instrument("mark_job_as_successful")
        .report_latency()
        .execute(self.storage.conn())
        .await
        .expect("failed to mark basic witness input producer job as successful");
    }

    pub async fn mark_job_as_failed(
        &mut self,
        l1_batch_number: L1BatchNumber,
        started_at: Instant,
        error: String,
    ) -> Option<u32> {
        sqlx::query!(
            "UPDATE basic_witness_input_producer_jobs
            SET status = $1, updated_at = now(), time_taken = $3, error = $4
            WHERE l1_batch_number = $2
            RETURNING basic_witness_input_producer_jobs.attempts",
            format!("{}", BasicWitnessInputProducerStatus::Successful),
            l1_batch_number.0 as i64,
            duration_to_naive_time(started_at.elapsed()),
            error
        )
        .instrument("mark_job_as_failed")
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await
        .expect("failed to mark basic witness input producer job as failed")
        .map(|job| job.attempts as u32)
    }
}

/// These functions should only be used for tests.
impl BasicWitnessInputProducerDal<'_, '_> {
    pub async fn delete_all_jobs(&mut self) -> sqlx::Result<()> {
        sqlx::query!("DELETE FROM basic_witness_input_producer_jobs")
            .execute(self.storage.conn())
            .await?;
        Ok(())
    }
}
