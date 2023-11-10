use sqlx::Row;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use strum::{Display, EnumString};

use zksync_types::proofs::{JobCountStatistics, StuckJobs};
use zksync_types::L1BatchNumber;

use crate::time_utils::{duration_to_naive_time, pg_interval_from_duration};
use crate::StorageProcessor;

#[derive(Debug)]
pub struct FriProofCompressorDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

#[derive(Debug, EnumString, Display)]
pub enum ProofCompressionJobStatus {
    #[strum(serialize = "queued")]
    Queued,
    #[strum(serialize = "in_progress")]
    InProgress,
    #[strum(serialize = "successful")]
    Successful,
    #[strum(serialize = "failed")]
    Failed,
    #[strum(serialize = "sent_to_server")]
    SentToServer,
    #[strum(serialize = "skipped")]
    Skipped,
}

impl FriProofCompressorDal<'_, '_> {
    pub async fn insert_proof_compression_job(
        &mut self,
        block_number: L1BatchNumber,
        fri_proof_blob_url: &str,
    ) {
        sqlx::query!(
                "INSERT INTO proof_compression_jobs_fri(l1_batch_number, fri_proof_blob_url, status, created_at, updated_at) \
                 VALUES ($1, $2, $3, now(), now()) \
                 ON CONFLICT (l1_batch_number) DO NOTHING",
                block_number.0 as i64,
                fri_proof_blob_url,
            ProofCompressionJobStatus::Queued.to_string(),
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();
    }

    pub async fn skip_proof_compression_job(&mut self, block_number: L1BatchNumber) {
        sqlx::query!(
                "INSERT INTO proof_compression_jobs_fri(l1_batch_number, status, created_at, updated_at) \
                 VALUES ($1, $2, now(), now()) \
                 ON CONFLICT (l1_batch_number) DO NOTHING",
                block_number.0 as i64,
            ProofCompressionJobStatus::Skipped.to_string(),
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();
    }

    pub async fn get_next_proof_compression_job(
        &mut self,
        picked_by: &str,
    ) -> Option<L1BatchNumber> {
        sqlx::query!(
            "UPDATE proof_compression_jobs_fri \
                SET status = $1, attempts = attempts + 1, \
                    updated_at = now(), processing_started_at = now(), \
                    picked_by = $3 \
                WHERE l1_batch_number = ( \
                    SELECT l1_batch_number \
                    FROM proof_compression_jobs_fri \
                    WHERE status = $2 \
                    ORDER BY l1_batch_number ASC \
                    LIMIT 1 \
                    FOR UPDATE \
                    SKIP LOCKED \
                ) \
                RETURNING proof_compression_jobs_fri.l1_batch_number",
            ProofCompressionJobStatus::InProgress.to_string(),
            ProofCompressionJobStatus::Queued.to_string(),
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
    }

    pub async fn get_proof_compression_job_attempts(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            "SELECT attempts FROM proof_compression_jobs_fri \
            WHERE l1_batch_number = $1",
            l1_batch_number.0 as i64,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.attempts as u32);

        Ok(attempts)
    }

    pub async fn mark_proof_compression_job_successful(
        &mut self,
        block_number: L1BatchNumber,
        time_taken: Duration,
        l1_proof_blob_url: &str,
    ) {
        sqlx::query!(
            "UPDATE proof_compression_jobs_fri \
             SET status = $1, updated_at = now(), time_taken = $2, l1_proof_blob_url = $3\
             WHERE l1_batch_number = $4",
            ProofCompressionJobStatus::Successful.to_string(),
            duration_to_naive_time(time_taken),
            l1_proof_blob_url,
            block_number.0 as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn mark_proof_compression_job_failed(
        &mut self,
        error: &str,
        block_number: L1BatchNumber,
    ) {
        sqlx::query!(
            "UPDATE proof_compression_jobs_fri \
            SET status =$1, error= $2, updated_at = now() \
            WHERE l1_batch_number = $3",
            ProofCompressionJobStatus::Failed.to_string(),
            error,
            block_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_least_proven_block_number_not_sent_to_server(
        &mut self,
    ) -> Option<(L1BatchNumber, ProofCompressionJobStatus)> {
        let row = sqlx::query!(
            "SELECT l1_batch_number, status \
            FROM proof_compression_jobs_fri
            WHERE l1_batch_number = ( \
                SELECT MIN(l1_batch_number) \
                FROM proof_compression_jobs_fri \
                WHERE status = $1 OR status = $2
            )",
            ProofCompressionJobStatus::Successful.to_string(),
            ProofCompressionJobStatus::Skipped.to_string()
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?;
        match row {
            Some(row) => Some((
                L1BatchNumber(row.l1_batch_number as u32),
                ProofCompressionJobStatus::from_str(&row.status).unwrap(),
            )),
            None => None,
        }
    }

    pub async fn mark_proof_sent_to_server(&mut self, block_number: L1BatchNumber) {
        sqlx::query!(
            "UPDATE proof_compression_jobs_fri \
            SET status = $1, updated_at = now() \
            WHERE l1_batch_number = $2",
            ProofCompressionJobStatus::SentToServer.to_string(),
            block_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_jobs_stats(&mut self) -> JobCountStatistics {
        let mut results: HashMap<String, i64> = sqlx::query(
            "SELECT COUNT(*) as \"count\", status as \"status\" \
                 FROM proof_compression_jobs_fri \
                 GROUP BY status",
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.get("status"), row.get::<i64, &str>("count")))
        .collect::<HashMap<String, i64>>();

        JobCountStatistics {
            queued: results.remove("queued").unwrap_or(0i64) as usize,
            in_progress: results.remove("in_progress").unwrap_or(0i64) as usize,
            failed: results.remove("failed").unwrap_or(0i64) as usize,
            successful: results.remove("successful").unwrap_or(0i64) as usize,
        }
    }

    pub async fn requeue_stuck_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        {
            sqlx::query!(
                "UPDATE proof_compression_jobs_fri \
                SET status = 'queued', updated_at = now(), processing_started_at = now() \
                WHERE (status = 'in_progress' AND  processing_started_at <= now() - $1::interval AND attempts < $2) \
                OR (status = 'failed' AND attempts < $2) \
                RETURNING l1_batch_number, status, attempts",
                &processing_timeout,
                max_attempts as i32,
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .map(|row| StuckJobs { id: row.l1_batch_number as u64, status: row.status, attempts: row.attempts as u64 })
                .collect()
        }
    }
}
