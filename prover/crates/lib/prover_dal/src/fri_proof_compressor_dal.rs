#![doc = include_str!("../doc/FriProofCompressorDal.md")]
use std::{collections::HashMap, str::FromStr, time::Duration};

use sqlx::types::chrono::{DateTime, Utc};
use zksync_basic_types::{
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    prover_dal::{
        JobCountStatistics, ProofCompressionJobInfo, ProofCompressionJobStatus, StuckJobs,
    },
    L1BatchId, L2ChainId,
};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::{duration_to_naive_time, pg_interval_from_duration, Prover};

#[derive(Debug)]
pub struct FriProofCompressorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriProofCompressorDal<'_, '_> {
    pub async fn insert_proof_compression_job(
        &mut self,
        batch_id: L1BatchId,
        fri_proof_blob_url: &str,
        protocol_version: ProtocolSemanticVersion,
        batch_sealed_at: DateTime<Utc>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            proof_compression_jobs_fri (
                l1_batch_number,
                chain_id,
                fri_proof_blob_url,
                status,
                created_at,
                updated_at,
                protocol_version,
                protocol_version_patch,
                batch_sealed_at
            )
            VALUES
            ($1, $2, $3, 'queued', NOW(), NOW(), $4, $5, $6)
            ON CONFLICT (l1_batch_number, chain_id) DO NOTHING
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
            fri_proof_blob_url,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            batch_sealed_at.naive_utc(),
        )
        .instrument("insert_proof_compression_job")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_next_proof_compression_job(
        &mut self,
        picked_by: &str,
        protocol_version: ProtocolSemanticVersion,
    ) -> Option<L1BatchId> {
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
                (l1_batch_number, chain_id) IN (
                    SELECT
                        l1_batch_number,
                        chain_id
                    FROM
                        proof_compression_jobs_fri
                    WHERE
                        status = $2
                        AND protocol_version = $4
                        AND protocol_version_patch = $5
                    ORDER BY
                        priority DESC,
                        batch_sealed_at ASC
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            proof_compression_jobs_fri.l1_batch_number,
            proof_compression_jobs_fri.chain_id
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
        .map(|row| L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32))
    }

    pub async fn get_proof_compression_job_attempts(
        &mut self,
        batch_id: L1BatchId,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                proof_compression_jobs_fri
            WHERE
                l1_batch_number = $1
                AND chain_id = $2
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.attempts as u32);

        Ok(attempts)
    }

    pub async fn mark_proof_compression_job_successful(
        &mut self,
        batch_id: L1BatchId,
        time_taken: Duration,
        l1_proof_blob_url: &str,
    ) -> DalResult<()> {
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
                AND chain_id = $5
            "#,
            ProofCompressionJobStatus::Successful.to_string(),
            duration_to_naive_time(time_taken),
            l1_proof_blob_url,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .instrument("mark_proof_compression_job_successful")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn mark_proof_compression_job_failed(
        &mut self,
        error: &str,
        batch_id: L1BatchId,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE proof_compression_jobs_fri
            SET
                status = $1,
                error = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $3
                AND chain_id = $4
                AND status != $5
                AND status != $6
            "#,
            ProofCompressionJobStatus::Failed.to_string(),
            error,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
            ProofCompressionJobStatus::Successful.to_string(),
            ProofCompressionJobStatus::SentToServer.to_string(),
        )
        .instrument("mark_proof_compression_job_failed")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    // todo: this should be grouped by chain_id
    pub async fn get_least_proven_block_not_sent_to_server(
        &mut self,
    ) -> Option<(
        L1BatchId,
        ProtocolSemanticVersion,
        ProofCompressionJobStatus,
    )> {
        let row = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                chain_id,
                status,
                protocol_version,
                protocol_version_patch
            FROM
                proof_compression_jobs_fri
            WHERE
                status IN ($1, $2)
            ORDER BY
                batch_sealed_at ASC
            LIMIT
                1
            "#,
            ProofCompressionJobStatus::Successful.to_string(),
            ProofCompressionJobStatus::Skipped.to_string()
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?;
        match row {
            Some(row) => Some((
                L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32),
                ProtocolSemanticVersion::new(
                    ProtocolVersionId::try_from(row.protocol_version.unwrap() as u16).unwrap(),
                    VersionPatch(row.protocol_version_patch as u32),
                ),
                ProofCompressionJobStatus::from_str(&row.status).unwrap(),
            )),
            None => None,
        }
    }

    pub async fn mark_proof_sent_to_server(&mut self, batch_id: L1BatchId) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE proof_compression_jobs_fri
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND chain_id = $3
            "#,
            ProofCompressionJobStatus::SentToServer.to_string(),
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .instrument("mark_proof_sent_to_server")
        .execute(self.storage)
        .await?;

        Ok(())
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

    pub async fn get_oldest_not_compressed_batch(&mut self) -> Option<L1BatchId> {
        let result: Option<L1BatchId> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                chain_id
            FROM
                proof_compression_jobs_fri
            WHERE
                status <> 'successful'
                AND status <> 'sent_to_server'
            ORDER BY
                batch_sealed_at DESC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32));

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
                chain_id,
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
                chain_id: L2ChainId::new(row.chain_id as u64).unwrap(),
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
        batch_id: L1BatchId,
    ) -> Option<ProofCompressionJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                proof_compression_jobs_fri
            WHERE
                l1_batch_number = $1
                AND chain_id = $2
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| ProofCompressionJobInfo {
            batch_id,
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

    pub async fn delete_batch_data(&mut self, batch_id: L1BatchId) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM proof_compression_jobs_fri
            WHERE
                l1_batch_number = $1
                AND chain_id = $2
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .instrument("delete_batch_data")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    // todo: THIS LOOK VERY BAD
    pub async fn delete(&mut self) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM proof_compression_jobs_fri
            "#
        )
        .instrument("delete")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn requeue_stuck_jobs_for_batch(
        &mut self,
        batch_id: L1BatchId,
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
                    AND chain_id = $2
                    AND attempts >= $3
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
                batch_id.batch_number().0 as i64,
                batch_id.chain_id().inner() as i64,
                max_attempts as i32,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: batch_id.batch_number().0 as u64,
                chain_id: batch_id.chain_id(),
                status: row.status,
                attempts: row.attempts as u64,
                circuit_id: None,
                error: row.error,
                picked_by: row.picked_by,
            })
            .collect()
        }
    }

    pub async fn check_reached_max_attempts(&mut self, max_attempts: u32) -> usize {
        sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)
            FROM proof_compression_jobs_fri
            WHERE
                attempts >= $1
                AND status <> 'successful'
                AND status <> 'sent_to_server'
            "#,
            max_attempts as i64
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .unwrap_or(0) as usize
    }
}
