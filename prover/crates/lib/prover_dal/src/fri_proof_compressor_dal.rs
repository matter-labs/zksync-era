#![doc = include_str!("../doc/FriProofCompressorDal.md")]
use std::{collections::HashMap, str::FromStr, time::Duration};

use zksync_basic_types::{
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    prover_dal::{
        JobCountStatistics, ProofCompressionJobInfo, ProofCompressionJobStatus, StuckJobs,
    },
    L1BatchNumber,
};
use zksync_db_connection::connection::Connection;

use crate::{duration_to_naive_time, pg_interval_from_duration, Prover};

#[derive(Debug)]
pub struct FriProofCompressorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriProofCompressorDal<'_, '_> {
    pub async fn insert_proof_compression_job(
        &mut self,
        block_number: L1BatchNumber,
        fri_proof_blob_url: &str,
        protocol_version: ProtocolSemanticVersion,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
                proof_compression_jobs_fri (
                    l1_batch_number,
                    fri_proof_blob_url,
                    status,
                    created_at,
                    updated_at,
                    protocol_version,
                    protocol_version_patch
                )
            VALUES
                ($1, $2, $3, NOW(), NOW(), $4, $5)
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(block_number.0),
            fri_proof_blob_url,
            ProofCompressionJobStatus::Queued.to_string(),
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_next_proof_compression_job(
        &mut self,
        picked_by: &str,
        protocol_version: ProtocolSemanticVersion,
    ) -> Option<L1BatchNumber> {
        sqlx::query!(
            r#"
            UPDATE proof_compression_jobs_fri
            SET
                status = $1,
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $3
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        proof_compression_jobs_fri
                    WHERE
                        status = $2
                        AND protocol_version = $4
                        AND protocol_version_patch = $5
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                proof_compression_jobs_fri.l1_batch_number
            "#,
            ProofCompressionJobStatus::InProgress.to_string(),
            ProofCompressionJobStatus::Queued.to_string(),
            picked_by,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32
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
            r#"
            SELECT
                attempts
            FROM
                proof_compression_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
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
            r#"
            UPDATE proof_compression_jobs_fri
            SET
                status = $1,
                updated_at = NOW(),
                time_taken = $2,
                l1_proof_blob_url = $3
            WHERE
                l1_batch_number = $4
            "#,
            ProofCompressionJobStatus::Successful.to_string(),
            duration_to_naive_time(time_taken),
            l1_proof_blob_url,
            i64::from(block_number.0)
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
            r#"
            UPDATE proof_compression_jobs_fri
            SET
                status = $1,
                error = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $3
                AND status != $4
                AND status != $5
            "#,
            ProofCompressionJobStatus::Failed.to_string(),
            error,
            i64::from(block_number.0),
            ProofCompressionJobStatus::Successful.to_string(),
            ProofCompressionJobStatus::SentToServer.to_string(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_least_proven_block_not_sent_to_server(
        &mut self,
    ) -> Option<(
        L1BatchNumber,
        ProtocolSemanticVersion,
        ProofCompressionJobStatus,
    )> {
        let row = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                status,
                protocol_version,
                protocol_version_patch
            FROM
                proof_compression_jobs_fri
            WHERE
                l1_batch_number = (
                    SELECT
                        MIN(l1_batch_number)
                    FROM
                        proof_compression_jobs_fri
                    WHERE
                        status = $1
                        OR status = $2
                )
            "#,
            ProofCompressionJobStatus::Successful.to_string(),
            ProofCompressionJobStatus::Skipped.to_string()
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?;
        match row {
            Some(row) => Some((
                L1BatchNumber(row.l1_batch_number as u32),
                ProtocolSemanticVersion::new(
                    ProtocolVersionId::try_from(row.protocol_version.unwrap() as u16).unwrap(),
                    VersionPatch(row.protocol_version_patch as u32),
                ),
                ProofCompressionJobStatus::from_str(&row.status).unwrap(),
            )),
            None => None,
        }
    }

    pub async fn mark_proof_sent_to_server(&mut self, block_number: L1BatchNumber) {
        sqlx::query!(
            r#"
            UPDATE proof_compression_jobs_fri
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            ProofCompressionJobStatus::SentToServer.to_string(),
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_jobs_stats(&mut self) -> HashMap<ProtocolSemanticVersion, JobCountStatistics> {
        sqlx::query!(
            r#"
            SELECT
                protocol_version,
                protocol_version_patch,
                COUNT(*) FILTER (
                    WHERE
                        status = 'queued'
                ) AS queued,
                COUNT(*) FILTER (
                    WHERE
                        status = 'in_progress'
                ) AS in_progress
            FROM
                proof_compression_jobs_fri
            WHERE
                protocol_version IS NOT NULL
            GROUP BY
                protocol_version,
                protocol_version_patch
            "#,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            let key = ProtocolSemanticVersion::new(
                ProtocolVersionId::try_from(row.protocol_version.unwrap() as u16).unwrap(),
                VersionPatch(row.protocol_version_patch as u32),
            );
            let value = JobCountStatistics {
                queued: row.queued.unwrap() as usize,
                in_progress: row.in_progress.unwrap() as usize,
            };
            (key, value)
        })
        .collect()
    }

    pub async fn get_oldest_not_compressed_batch(&mut self) -> Option<L1BatchNumber> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                proof_compression_jobs_fri
            WHERE
                status <> 'successful'
                AND status <> 'sent_to_server'
            ORDER BY
                l1_batch_number ASC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        result
    }

    pub async fn requeue_stuck_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        {
            sqlx::query!(
                r#"
                UPDATE proof_compression_jobs_fri
                SET
                    status = 'queued',
                    updated_at = NOW(),
                    processing_started_at = NOW()
                WHERE
                    (
                        status = 'in_progress'
                        AND processing_started_at <= NOW() - $1::INTERVAL
                        AND attempts < $2
                    )
                    OR (
                        status = 'failed'
                        AND attempts < $2
                    )
                RETURNING
                    l1_batch_number,
                    status,
                    attempts,
                    error,
                    picked_by
                "#,
                &processing_timeout,
                max_attempts as i32,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: row.l1_batch_number as u64,
                status: row.status,
                attempts: row.attempts as u64,
                circuit_id: None,
                error: row.error,
                picked_by: row.picked_by,
            })
            .collect()
        }
    }

    pub async fn get_proof_compression_job_for_batch(
        &mut self,
        block_number: L1BatchNumber,
    ) -> Option<ProofCompressionJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                proof_compression_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(block_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| ProofCompressionJobInfo {
            l1_batch_number: block_number,
            attempts: row.attempts as u32,
            status: ProofCompressionJobStatus::from_str(&row.status).unwrap(),
            fri_proof_blob_url: row.fri_proof_blob_url,
            l1_proof_blob_url: row.l1_proof_blob_url,
            error: row.error,
            created_at: row.created_at,
            updated_at: row.updated_at,
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            picked_by: row.picked_by,
        })
    }

    pub async fn delete_batch_data(
        &mut self,
        block_number: L1BatchNumber,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM proof_compression_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete(&mut self) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM proof_compression_jobs_fri
            "#
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn requeue_stuck_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        {
            sqlx::query!(
                r#"
                UPDATE proof_compression_jobs_fri
                SET
                    status = 'queued',
                    error = 'Manually requeued',
                    attempts = 2,
                    updated_at = NOW(),
                    processing_started_at = NOW()
                WHERE
                    l1_batch_number = $1
                    AND attempts >= $2
                    AND (
                        status = 'in_progress'
                        OR status = 'failed'
                    )
                RETURNING
                    status,
                    attempts,
                    error,
                    picked_by
                "#,
                i64::from(block_number.0),
                max_attempts as i32,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: block_number.0 as u64,
                status: row.status,
                attempts: row.attempts as u64,
                circuit_id: None,
                error: row.error,
                picked_by: row.picked_by,
            })
            .collect()
        }
    }
}
