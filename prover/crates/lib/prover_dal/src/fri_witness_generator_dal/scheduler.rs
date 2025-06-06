use std::{str::FromStr, time::Duration};

use sqlx::types::chrono::{DateTime, Utc};
use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::ProtocolSemanticVersion,
    prover_dal::{SchedulerWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus},
    L1BatchId, L2ChainId,
};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::InstrumentExt as _,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::Prover;

#[derive(Debug)]
pub struct FriSchedulerWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriSchedulerWitnessGeneratorDal<'_, '_> {
    pub async fn move_scheduler_jobs_from_waiting_to_queued(&mut self) -> Vec<L1BatchId> {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                (l1_batch_number, chain_id) IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.chain_id
                    FROM
                        prover_jobs_fri
                    JOIN
                        scheduler_witness_jobs_fri swj
                        ON
                            prover_jobs_fri.l1_batch_number = swj.l1_batch_number
                            AND prover_jobs_fri.chain_id = swj.chain_id
                    WHERE
                        swj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = $1
                )
            RETURNING
            l1_batch_number,
            chain_id;
            "#,
            AggregationRound::RecursionTip as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32))
        .collect()
    }

    pub async fn mark_scheduler_jobs_as_queued(&mut self, batch_id: L1BatchId) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                l1_batch_number = $1
                AND chain_id = $2
                AND status != 'successful'
                AND status != 'in_progress'
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .instrument("mark_scheduler_jobs_as_queued")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn requeue_stuck_scheduler_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
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

    pub async fn get_next_scheduler_witness_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<L1BatchId> {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $2
            WHERE
                (l1_batch_number, chain_id) IN (
                    SELECT
                        l1_batch_number,
                        chain_id
                    FROM
                        scheduler_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $3
                    ORDER BY
                        priority DESC,
                        batch_sealed_at ASC
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            scheduler_witness_jobs_fri.*
            "#,
            protocol_version.minor as i32,
            picked_by,
            protocol_version.patch.0 as i32,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32))
    }

    pub async fn mark_scheduler_job_as_successful(
        &mut self,
        batch_id: L1BatchId,
        time_taken: Duration,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1
            WHERE
                l1_batch_number = $2
                AND chain_id = $3
            "#,
            duration_to_naive_time(time_taken),
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .instrument("mark_scheduler_job_as_successful")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_scheduler_witness_generator_jobs_for_batch(
        &mut self,
        batch_id: L1BatchId,
    ) -> Option<SchedulerWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                scheduler_witness_jobs_fri
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
        .map(|row| SchedulerWitnessGeneratorJobInfo {
            batch_id,
            scheduler_partial_input_blob_url: row.scheduler_partial_input_blob_url.clone(),
            status: WitnessJobStatus::from_str(&row.status).unwrap(),
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            error: row.error.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            attempts: row.attempts as u32,
            protocol_version: row.protocol_version,
            picked_by: row.picked_by.clone(),
        })
    }

    pub async fn requeue_stuck_scheduler_jobs_for_batch(
        &mut self,
        batch_id: L1BatchId,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'queued',
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
            l1_batch_number,
            chain_id,
            status,
            attempts,
            error,
            picked_by
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
            max_attempts as i64
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

    pub async fn insert_scheduler_aggregation_jobs(
        &mut self,
        batch_id: L1BatchId,
        scheduler_partial_input_blob_url: &str,
        protocol_version: ProtocolSemanticVersion,
        batch_sealed_at: DateTime<Utc>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            scheduler_witness_jobs_fri (
                l1_batch_number,
                chain_id,
                scheduler_partial_input_blob_url,
                protocol_version,
                status,
                created_at,
                updated_at,
                protocol_version_patch,
                batch_sealed_at
            )
            VALUES
            ($1, $2, $3, $4, 'waiting_for_proofs', NOW(), NOW(), $5, $6)
            ON CONFLICT (l1_batch_number, chain_id) DO
            UPDATE
            SET
            updated_at = NOW()
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
            scheduler_partial_input_blob_url,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            batch_sealed_at.naive_utc(),
        )
        .instrument("insert_scheduler_aggregation_jobs")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn check_reached_max_attempts(&mut self, max_attempts: u32) -> usize {
        sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)
            FROM scheduler_witness_jobs_fri
            WHERE
                attempts >= $1
                AND status <> 'successful'
            "#,
            max_attempts as i64
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .unwrap_or(0) as usize
    }
}
