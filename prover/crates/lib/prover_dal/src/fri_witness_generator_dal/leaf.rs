use std::{str::FromStr, time::Duration};

use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::ProtocolSemanticVersion,
    prover_dal::{
        LeafAggregationJobMetadata, LeafWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus,
    },
    L1BatchNumber,
};
use zksync_db_connection::{
    connection::Connection,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::{Prover, ProverDal};

#[derive(Debug)]
pub struct FriLeafWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriLeafWitnessGeneratorDal<'_, '_> {
    pub async fn mark_leaf_aggregation_as_successful(&mut self, id: u32, time_taken: Duration) {
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1
            WHERE
                id = $2
            "#,
            duration_to_naive_time(time_taken),
            i64::from(id)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_next_leaf_aggregation_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<LeafAggregationJobMetadata> {
        let row = sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $3
            WHERE
                id = (
                    SELECT
                        id
                    FROM
                        leaf_aggregation_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $2
                    ORDER BY
                        l1_batch_number ASC,
                        id ASC
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            leaf_aggregation_witness_jobs_fri.*
            "#,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;

        let block_number = L1BatchNumber(row.l1_batch_number as u32);
        let proof_job_ids = self
            .storage
            .fri_prover_jobs_dal()
            .prover_job_ids_for(
                block_number,
                row.circuit_id as u8,
                AggregationRound::BasicCircuits,
                0,
            )
            .await;
        Some(LeafAggregationJobMetadata {
            id: row.id as u32,
            block_number,
            circuit_id: row.circuit_id as u8,
            prover_job_ids_for_proofs: proof_job_ids,
        })
    }

    pub async fn move_leaf_aggregation_jobs_from_waiting_to_queued(&mut self) -> Vec<(i64, u8)> {
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                (l1_batch_number, circuit_id) IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.circuit_id
                    FROM
                        prover_jobs_fri
                    JOIN leaf_aggregation_witness_jobs_fri lawj
                        ON
                            prover_jobs_fri.l1_batch_number = lawj.l1_batch_number
                            AND prover_jobs_fri.circuit_id = lawj.circuit_id
                    WHERE
                        lawj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = 0
                    GROUP BY
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.circuit_id,
                        lawj.number_of_basic_circuits
                    HAVING
                        COUNT(*) = lawj.number_of_basic_circuits
                )
            RETURNING
            l1_batch_number,
            circuit_id;
            "#,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.l1_batch_number, row.circuit_id as u8))
        .collect()
    }

    pub async fn requeue_stuck_leaf_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
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
            id,
            status,
            attempts,
            circuit_id,
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
            id: row.id as u64,
            status: row.status,
            attempts: row.attempts as u64,
            circuit_id: Some(row.circuit_id as u32),
            error: row.error,
            picked_by: row.picked_by,
        })
        .collect()
    }

    pub async fn get_leaf_witness_generator_jobs_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Vec<LeafWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                leaf_aggregation_witness_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .iter()
        .map(|row| LeafWitnessGeneratorJobInfo {
            id: row.id as u32,
            l1_batch_number,
            circuit_id: row.circuit_id as u32,
            closed_form_inputs_blob_url: row.closed_form_inputs_blob_url.clone(),
            attempts: row.attempts as u32,
            status: WitnessJobStatus::from_str(&row.status).unwrap(),
            error: row.error.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            protocol_version: row.protocol_version,
            picked_by: row.picked_by.clone(),
            number_of_basic_circuits: row.number_of_basic_circuits,
        })
        .collect()
    }

    pub async fn insert_leaf_aggregation_jobs(
        &mut self,
        block_number: L1BatchNumber,
        protocol_version: ProtocolSemanticVersion,
        circuit_id: u8,
        closed_form_inputs_url: String,
        number_of_basic_circuits: usize,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            leaf_aggregation_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                closed_form_inputs_blob_url,
                number_of_basic_circuits,
                protocol_version,
                status,
                created_at,
                updated_at,
                protocol_version_patch
            )
            VALUES
            ($1, $2, $3, $4, $5, 'waiting_for_proofs', NOW(), NOW(), $6)
            ON CONFLICT (l1_batch_number, circuit_id) DO
            UPDATE
            SET
            updated_at = NOW()
            "#,
            i64::from(block_number.0),
            i16::from(circuit_id),
            closed_form_inputs_url,
            number_of_basic_circuits as i32,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
