use std::{str::FromStr, time::Duration};

use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::ProtocolSemanticVersion,
    prover_dal::{
        NodeAggregationJobMetadata, NodeWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus,
    },
    L1BatchNumber,
};
use zksync_db_connection::{
    connection::Connection,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::{Prover, ProverDal};

#[derive(Debug)]
pub struct FriNodeWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriNodeWitnessGeneratorDal<'_, '_> {
    pub async fn update_node_aggregation_jobs_url(
        &mut self,
        block_number: L1BatchNumber,
        circuit_id: u8,
        number_of_dependent_jobs: usize,
        depth: u16,
        url: String,
    ) {
        sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
            SET
                aggregations_url = $1,
                number_of_dependent_jobs = $5,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND circuit_id = $3
                AND depth = $4
            "#,
            url,
            i64::from(block_number.0),
            i16::from(circuit_id),
            i32::from(depth),
            number_of_dependent_jobs as i32,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_next_node_aggregation_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<NodeAggregationJobMetadata> {
        let row = sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
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
                        node_aggregation_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $2
                    ORDER BY
                        l1_batch_number ASC,
                        depth ASC,
                        id ASC
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            node_aggregation_witness_jobs_fri.*
            "#,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;
        let depth = row.depth as u16;

        let round = match depth {
            // Zero depth implies this is the first time we are performing node aggregation,
            // i.e we load proofs from previous round that is leaf aggregation.
            0 => AggregationRound::LeafAggregation,
            _ => AggregationRound::NodeAggregation,
        };

        let block_number = L1BatchNumber(row.l1_batch_number as u32);
        let prover_job_ids = self
            .storage
            .fri_prover_jobs_dal()
            .prover_job_ids_for(block_number, row.circuit_id as u8, round, depth)
            .await;
        Some(NodeAggregationJobMetadata {
            id: row.id as u32,
            block_number,
            circuit_id: row.circuit_id as u8,
            depth,
            prover_job_ids_for_proofs: prover_job_ids,
        })
    }

    pub async fn mark_node_aggregation_as_successful(&mut self, id: u32, time_taken: Duration) {
        sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
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

    pub async fn insert_node_aggregation_jobs(
        &mut self,
        block_number: L1BatchNumber,
        circuit_id: u8,
        number_of_dependent_jobs: Option<i32>,
        depth: u16,
        aggregations_url: &str,
        protocol_version: ProtocolSemanticVersion,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            node_aggregation_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                depth,
                aggregations_url,
                number_of_dependent_jobs,
                protocol_version,
                status,
                created_at,
                updated_at,
                protocol_version_patch
            )
            VALUES
            ($1, $2, $3, $4, $5, $6, 'waiting_for_proofs', NOW(), NOW(), $7)
            ON CONFLICT (l1_batch_number, circuit_id, depth) DO
            UPDATE
            SET
            updated_at = NOW()
            "#,
            i64::from(block_number.0),
            i16::from(circuit_id),
            i32::from(depth),
            aggregations_url,
            number_of_dependent_jobs,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn move_depth_zero_node_aggregation_jobs(&mut self) -> Vec<(i64, u8, u16)> {
        sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                (l1_batch_number, circuit_id, depth) IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.circuit_id,
                        prover_jobs_fri.depth
                    FROM
                        prover_jobs_fri
                    JOIN node_aggregation_witness_jobs_fri nawj
                        ON
                            prover_jobs_fri.l1_batch_number = nawj.l1_batch_number
                            AND prover_jobs_fri.circuit_id = nawj.circuit_id
                            AND prover_jobs_fri.depth = nawj.depth
                    WHERE
                        nawj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = 1
                        AND prover_jobs_fri.depth = 0
                    GROUP BY
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.circuit_id,
                        prover_jobs_fri.depth,
                        nawj.number_of_dependent_jobs
                    HAVING
                        COUNT(*) = nawj.number_of_dependent_jobs
                )
            RETURNING
            l1_batch_number,
            circuit_id,
            depth;
            "#,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.l1_batch_number, row.circuit_id as u8, row.depth as u16))
        .collect()
    }

    pub async fn move_depth_non_zero_node_aggregation_jobs(&mut self) -> Vec<(i64, u8, u16)> {
        sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                (l1_batch_number, circuit_id, depth) IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.circuit_id,
                        prover_jobs_fri.depth
                    FROM
                        prover_jobs_fri
                    JOIN node_aggregation_witness_jobs_fri nawj
                        ON
                            prover_jobs_fri.l1_batch_number = nawj.l1_batch_number
                            AND prover_jobs_fri.circuit_id = nawj.circuit_id
                            AND prover_jobs_fri.depth = nawj.depth
                    WHERE
                        nawj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = 2
                    GROUP BY
                        prover_jobs_fri.l1_batch_number,
                        prover_jobs_fri.circuit_id,
                        prover_jobs_fri.depth,
                        nawj.number_of_dependent_jobs
                    HAVING
                        COUNT(*) = nawj.number_of_dependent_jobs
                )
            RETURNING
            l1_batch_number,
            circuit_id,
            depth;
            "#,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.l1_batch_number, row.circuit_id as u8, row.depth as u16))
        .collect()
    }

    pub async fn requeue_stuck_node_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
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

    pub async fn get_node_witness_generator_jobs_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Vec<NodeWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                node_aggregation_witness_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .iter()
        .map(|row| NodeWitnessGeneratorJobInfo {
            id: row.id as u32,
            l1_batch_number,
            circuit_id: row.circuit_id as u32,
            depth: row.depth as u32,
            status: WitnessJobStatus::from_str(&row.status).unwrap(),
            attempts: row.attempts as u32,
            aggregations_url: row.aggregations_url.clone(),
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            error: row.error.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            number_of_dependent_jobs: row.number_of_dependent_jobs,
            protocol_version: row.protocol_version,
            picked_by: row.picked_by.clone(),
        })
        .collect()
    }
}
