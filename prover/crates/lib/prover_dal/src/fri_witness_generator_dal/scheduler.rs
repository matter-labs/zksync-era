use std::{str::FromStr, time::Duration};

use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::ProtocolSemanticVersion,
    prover_dal::{SchedulerWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus},
    L1BatchNumber,
};
use zksync_db_connection::{
    connection::Connection,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::Prover;

#[derive(Debug)]
pub struct FriSchedulerWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriSchedulerWitnessGeneratorDal<'_, '_> {
    pub async fn move_scheduler_jobs_from_waiting_to_queued(&mut self) -> Vec<u64> {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                l1_batch_number IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number
                    FROM
                        prover_jobs_fri
                    JOIN
                        scheduler_witness_jobs_fri swj
                        ON prover_jobs_fri.l1_batch_number = swj.l1_batch_number
                    WHERE
                        swj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = $1
                )
            RETURNING
            l1_batch_number;
            "#,
            AggregationRound::RecursionTip as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.l1_batch_number as u64))
        .collect()
    }

    pub async fn mark_scheduler_jobs_as_queued(&mut self, l1_batch_number: i64) {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                l1_batch_number = $1
                AND status != 'successful'
                AND status != 'in_progress'
            "#,
            l1_batch_number
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
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

    pub async fn get_next_scheduler_witness_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<L1BatchNumber> {
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
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        scheduler_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $3
                    ORDER BY
                        l1_batch_number ASC
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
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
    }

    pub async fn mark_scheduler_job_as_successful(
        &mut self,
        block_number: L1BatchNumber,
        time_taken: Duration,
    ) {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1
            WHERE
                l1_batch_number = $2
            "#,
            duration_to_naive_time(time_taken),
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_scheduler_witness_generator_jobs_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<SchedulerWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                scheduler_witness_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| SchedulerWitnessGeneratorJobInfo {
            l1_batch_number,
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
        block_number: L1BatchNumber,
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
                AND attempts >= $2
                AND (
                    status = 'in_progress'
                    OR status = 'failed'
                )
            RETURNING
            l1_batch_number,
            status,
            attempts,
            error,
            picked_by
            "#,
            i64::from(block_number.0),
            max_attempts as i64
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

    pub async fn insert_scheduler_aggregation_jobs(
        &mut self,
        block_number: L1BatchNumber,
        scheduler_partial_input_blob_url: &str,
        protocol_version: ProtocolSemanticVersion,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            scheduler_witness_jobs_fri (
                l1_batch_number,
                scheduler_partial_input_blob_url,
                protocol_version,
                status,
                created_at,
                updated_at,
                protocol_version_patch
            )
            VALUES
            ($1, $2, $3, 'waiting_for_proofs', NOW(), NOW(), $4)
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
            updated_at = NOW()
            "#,
            i64::from(block_number.0),
            scheduler_partial_input_blob_url,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
