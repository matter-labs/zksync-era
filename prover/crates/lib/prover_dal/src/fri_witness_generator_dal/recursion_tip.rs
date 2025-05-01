use std::{str::FromStr, time::Duration};

use sqlx::types::chrono::{DateTime, Utc};
use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::ProtocolSemanticVersion,
    prover_dal::{RecursionTipWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus},
    L1BatchId, L2ChainId,
};
use zksync_db_connection::{
    connection::Connection,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::Prover;

#[derive(Debug)]
pub struct FriRecursionTipWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriRecursionTipWitnessGeneratorDal<'_, '_> {
    pub async fn move_recursion_tip_jobs_from_waiting_to_queued(&mut self) -> Vec<L1BatchId> {
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
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
                        recursion_tip_witness_jobs_fri rtwj
                        ON
                            prover_jobs_fri.l1_batch_number = rtwj.l1_batch_number
                            AND prover_jobs_fri.chain_id = rtwj.chain_id
                    WHERE
                        rtwj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = $1
                        AND prover_jobs_fri.is_node_final_proof = TRUE
                    GROUP BY
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.chain_id,
                        rtwj.number_of_final_node_jobs
                    HAVING
                        COUNT(*) = rtwj.number_of_final_node_jobs
                )
            RETURNING
            l1_batch_number,
            chain_id;
            "#,
            AggregationRound::NodeAggregation as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32))
        .collect()
    }

    pub async fn requeue_stuck_recursion_tip_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW(),
                priority = priority + 1
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

    pub async fn get_next_recursion_tip_witness_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<(L1BatchId, i32)> {
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $3
            WHERE
                (l1_batch_number, chain_id) = (
                    SELECT
                        l1_batch_number,
                        chain_id
                    FROM
                        recursion_tip_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $2
                    ORDER BY
                        priority DESC,
                        batch_sealed_at ASC
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            recursion_tip_witness_jobs_fri.l1_batch_number,
            recursion_tip_witness_jobs_fri.chain_id,
            recursion_tip_witness_jobs_fri.number_of_final_node_jobs
            "#,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| {
            (
                L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32),
                row.number_of_final_node_jobs,
            )
        })
    }

    pub async fn mark_recursion_tip_job_as_successful(
        &mut self,
        batch_id: L1BatchId,
        time_taken: Duration,
    ) {
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1
            WHERE
                (l1_batch_number, chain_id) = (
                    $2,
                    $3
                )
            "#,
            duration_to_naive_time(time_taken),
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_recursion_tip_witness_generator_jobs_for_batch(
        &mut self,
        batch_id: L1BatchId,
    ) -> Option<RecursionTipWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                recursion_tip_witness_jobs_fri
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
        .map(|row| RecursionTipWitnessGeneratorJobInfo {
            batch_id,
            status: WitnessJobStatus::from_str(&row.status).unwrap(),
            attempts: row.attempts as u32,
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            error: row.error.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            number_of_final_node_jobs: row.number_of_final_node_jobs,
            protocol_version: row.protocol_version,
            picked_by: row.picked_by.clone(),
        })
    }

    pub async fn requeue_stuck_recursion_tip_jobs_for_batch(
        &mut self,
        batch_id: L1BatchId,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW(),
                priority = priority + 1
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

    pub async fn insert_recursion_tip_aggregation_jobs(
        &mut self,
        batch_id: L1BatchId,
        closed_form_inputs_and_urls: &[(u8, String, usize)],
        protocol_version: ProtocolSemanticVersion,
        batch_sealed_at: DateTime<Utc>,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            recursion_tip_witness_jobs_fri (
                l1_batch_number,
                chain_id,
                status,
                number_of_final_node_jobs,
                protocol_version,
                created_at,
                updated_at,
                protocol_version_patch,
                batch_sealed_at
            )
            VALUES
            ($1, $2, 'waiting_for_proofs', $3, $4, NOW(), NOW(), $5, $6)
            ON CONFLICT (l1_batch_number, chain_id) DO
            UPDATE
            SET
            updated_at = NOW()
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
            closed_form_inputs_and_urls.len() as i32,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            batch_sealed_at.naive_utc(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn check_reached_max_attempts(&mut self, max_attempts: u32) -> usize {
        sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)
            FROM recursion_tip_witness_jobs_fri
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
