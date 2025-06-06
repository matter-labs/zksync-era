use std::{str::FromStr, time::Duration};

use sqlx::types::chrono::{DateTime, Utc};
use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::ProtocolSemanticVersion,
    prover_dal::{
        LeafAggregationJobMetadata, LeafWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus,
    },
    L1BatchId, L2ChainId,
};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::InstrumentExt as _,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::{Prover, ProverDal};

#[derive(Debug)]
pub struct FriLeafWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriLeafWitnessGeneratorDal<'_, '_> {
    pub async fn mark_leaf_aggregation_as_successful(
        &mut self,
        id: u32,
        chain_id: L2ChainId,
        time_taken: Duration,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1
            WHERE
                id = $2
                AND chain_id = $3
            "#,
            duration_to_naive_time(time_taken),
            i64::from(id),
            chain_id.inner() as i64,
        )
        .instrument("mark_leaf_aggregation_as_successful")
        .execute(self.storage)
        .await?;

        Ok(())
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
                (id, chain_id) IN (
                    SELECT
                        id,
                        chain_id
                    FROM
                        leaf_aggregation_witness_jobs_fri
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
            leaf_aggregation_witness_jobs_fri.*
            "#,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;

        let batch_id = L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32);
        let proof_job_ids = self
            .storage
            .fri_prover_jobs_dal()
            .prover_job_ids_for(
                batch_id,
                row.circuit_id as u8,
                AggregationRound::BasicCircuits,
                0,
            )
            .await;
        Some(LeafAggregationJobMetadata {
            id: row.id as u32,
            batch_id,
            circuit_id: row.circuit_id as u8,
            prover_job_ids_for_proofs: proof_job_ids,
        })
    }

    pub async fn move_leaf_aggregation_jobs_from_waiting_to_queued(
        &mut self,
    ) -> Vec<(L1BatchId, u8)> {
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                (l1_batch_number, chain_id, circuit_id) IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.chain_id,
                        prover_jobs_fri.circuit_id
                    FROM
                        prover_jobs_fri
                    JOIN leaf_aggregation_witness_jobs_fri lawj
                        ON
                            prover_jobs_fri.l1_batch_number = lawj.l1_batch_number
                            AND prover_jobs_fri.circuit_id = lawj.circuit_id
                            AND prover_jobs_fri.chain_id = lawj.chain_id
                    WHERE
                        lawj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = 0
                    GROUP BY
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.chain_id,
                        prover_jobs_fri.circuit_id,
                        lawj.number_of_basic_circuits
                    HAVING
                        COUNT(*) = lawj.number_of_basic_circuits
                )
            RETURNING
            l1_batch_number,
            chain_id,
            circuit_id;
            "#,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32),
                row.circuit_id as u8,
            )
        })
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
            chain_id,
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
            chain_id: L2ChainId::new(row.chain_id as u64).unwrap(),
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
        batch_id: L1BatchId,
    ) -> Vec<LeafWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                leaf_aggregation_witness_jobs_fri
            WHERE
                l1_batch_number = $1
                AND chain_id = $2
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .iter()
        .map(|row| LeafWitnessGeneratorJobInfo {
            id: row.id as u32,
            batch_id: L1BatchId::from_raw(row.chain_id as u64, row.l1_batch_number as u32),
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
        batch_id: L1BatchId,
        protocol_version: ProtocolSemanticVersion,
        circuit_id: u8,
        closed_form_inputs_url: String,
        number_of_basic_circuits: usize,
        batch_sealed_at: DateTime<Utc>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            leaf_aggregation_witness_jobs_fri (
                l1_batch_number,
                chain_id,
                circuit_id,
                closed_form_inputs_blob_url,
                number_of_basic_circuits,
                protocol_version,
                status,
                created_at,
                updated_at,
                protocol_version_patch,
                batch_sealed_at
            )
            VALUES
            (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                'waiting_for_proofs',
                NOW(),
                NOW(),
                $7,
                $8
            )
            ON CONFLICT (l1_batch_number, chain_id, circuit_id) DO
            UPDATE
            SET
            updated_at = NOW()
            "#,
            batch_id.batch_number().0 as i64,
            batch_id.chain_id().inner() as i64,
            i16::from(circuit_id),
            closed_form_inputs_url,
            number_of_basic_circuits as i32,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            batch_sealed_at.naive_utc()
        )
        .instrument("insert_leaf_aggregation_jobs")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn check_reached_max_attempts(&mut self, max_attempts: u32) -> usize {
        sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)
            FROM leaf_aggregation_witness_jobs_fri
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
