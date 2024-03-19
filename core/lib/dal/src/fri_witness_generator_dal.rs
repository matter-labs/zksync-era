#![doc = include_str!("../doc/FriWitnessGeneratorDal.md")]
use std::{collections::HashMap, convert::TryFrom, time::Duration};

use sqlx::Row;
use zksync_types::{
    basic_fri_types::{AggregationRound, Eip4844Blobs},
    protocol_version::FriProtocolVersionId,
    L1BatchNumber,
};

use crate::{
    fri_prover_dal::types::{
        JobCountStatistics, LeafAggregationJobMetadata, NodeAggregationJobMetadata, StuckJobs,
    },
    metrics::MethodLatency,
    time_utils::{duration_to_naive_time, pg_interval_from_duration},
    StorageProcessor,
};

#[derive(Debug)]
pub struct FriWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

#[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
pub enum FriWitnessJobStatus {
    #[strum(serialize = "failed")]
    Failed,
    #[strum(serialize = "skipped")]
    Skipped,
    #[strum(serialize = "successful")]
    Successful,
    #[strum(serialize = "in_progress")]
    InProgress,
    #[strum(serialize = "queued")]
    Queued,
}

impl FriWitnessGeneratorDal<'_, '_> {
    pub async fn save_witness_inputs(
        &mut self,
        block_number: L1BatchNumber,
        object_key: &str,
        protocol_version_id: FriProtocolVersionId,
        eip_4844_blobs: Eip4844Blobs,
    ) {
        let blobs_raw: Vec<u8> = eip_4844_blobs.into();
        sqlx::query!(
            r#"
            INSERT INTO
                witness_inputs_fri (
                    l1_batch_number,
                    merkle_tree_paths_blob_url,
                    protocol_version,
                    eip_4844_blobs,
                    status,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3, $4, 'queued', NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(block_number.0),
            object_key,
            protocol_version_id as i32,
            blobs_raw,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap();
    }

    /// Gets the next job to be executed. Returns the batch number and its corresponding blobs.
    /// The blobs arrive from core via prover gateway, as pubdata, this method loads the blobs.
    pub async fn get_next_basic_circuit_witness_job(
        &mut self,
        last_l1_batch_to_process: u32,
        protocol_versions: &[FriProtocolVersionId],
        picked_by: &str,
    ) -> Option<(L1BatchNumber, Eip4844Blobs)> {
        let protocol_versions: Vec<i32> = protocol_versions.iter().map(|&id| id as i32).collect();
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $3
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        witness_inputs_fri
                    WHERE
                        l1_batch_number <= $1
                        AND status = 'queued'
                        AND protocol_version = ANY ($2)
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                witness_inputs_fri.*
            "#,
            i64::from(last_l1_batch_to_process),
            &protocol_versions[..],
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| {
            (
                L1BatchNumber(row.l1_batch_number as u32),
                row.eip_4844_blobs
                    .expect("missing eip 4844 blobs from the database")
                    .into(),
            )
        })
    }

    pub async fn get_basic_circuit_witness_job_attempts(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                witness_inputs_fri
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

    pub async fn mark_witness_job(
        &mut self,
        status: FriWitnessJobStatus,
        block_number: L1BatchNumber,
    ) {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            status.to_string(),
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn mark_witness_job_as_successful(
        &mut self,
        block_number: L1BatchNumber,
        time_taken: Duration,
    ) {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
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

    pub async fn mark_witness_job_failed(&mut self, error: &str, block_number: L1BatchNumber) {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'failed',
                error = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            error,
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn mark_leaf_aggregation_job_failed(&mut self, error: &str, id: u32) {
        sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'failed',
                error = $1,
                updated_at = NOW()
            WHERE
                id = $2
            "#,
            error,
            i64::from(id)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

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

    pub async fn requeue_stuck_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
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
                    status = 'in_gpu_proof'
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
                attempts
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
        })
        .collect()
    }

    pub async fn create_aggregation_jobs(
        &mut self,
        block_number: L1BatchNumber,
        closed_form_inputs_and_urls: &Vec<(u8, String, usize)>,
        scheduler_partial_input_blob_url: &str,
        base_layer_to_recursive_layer_circuit_id: fn(u8) -> u8,
        protocol_version_id: FriProtocolVersionId,
    ) {
        {
            let latency = MethodLatency::new("create_aggregation_jobs_fri");
            for (circuit_id, closed_form_inputs_url, number_of_basic_circuits) in
                closed_form_inputs_and_urls
            {
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
                            updated_at
                        )
                    VALUES
                        ($1, $2, $3, $4, $5, 'waiting_for_proofs', NOW(), NOW())
                    ON CONFLICT (l1_batch_number, circuit_id) DO
                    UPDATE
                    SET
                        updated_at = NOW()
                    "#,
                    i64::from(block_number.0),
                    i16::from(*circuit_id),
                    closed_form_inputs_url,
                    *number_of_basic_circuits as i32,
                    protocol_version_id as i32,
                )
                .execute(self.storage.conn())
                .await
                .unwrap();

                self.insert_node_aggregation_jobs(
                    block_number,
                    base_layer_to_recursive_layer_circuit_id(*circuit_id),
                    None,
                    0,
                    "",
                    protocol_version_id,
                )
                .await;
            }

            sqlx::query!(
                r#"
                INSERT INTO
                    scheduler_witness_jobs_fri (
                        l1_batch_number,
                        scheduler_partial_input_blob_url,
                        protocol_version,
                        status,
                        created_at,
                        updated_at
                    )
                VALUES
                    ($1, $2, $3, 'waiting_for_proofs', NOW(), NOW())
                ON CONFLICT (l1_batch_number) DO
                UPDATE
                SET
                    updated_at = NOW()
                "#,
                i64::from(block_number.0),
                scheduler_partial_input_blob_url,
                protocol_version_id as i32,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();

            sqlx::query!(
                r#"
                INSERT INTO
                    scheduler_dependency_tracker_fri (l1_batch_number, status, created_at, updated_at)
                VALUES
                    ($1, 'waiting_for_proofs', NOW(), NOW())
                ON CONFLICT (l1_batch_number) DO
                UPDATE
                SET
                    updated_at = NOW()
                "#,
                i64::from(block_number.0)
            )
            .execute(self.storage.conn())
            .await
            .unwrap();

            drop(latency);
        }
    }

    pub async fn get_next_leaf_aggregation_job(
        &mut self,
        protocol_versions: &[FriProtocolVersionId],
        picked_by: &str,
    ) -> Option<LeafAggregationJobMetadata> {
        let protocol_versions: Vec<i32> = protocol_versions.iter().map(|&id| id as i32).collect();
        let row = sqlx::query!(
            r#"
            UPDATE leaf_aggregation_witness_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $2
            WHERE
                id = (
                    SELECT
                        id
                    FROM
                        leaf_aggregation_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = ANY ($1)
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
            &protocol_versions[..],
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;

        let block_number = L1BatchNumber(row.l1_batch_number as u32);
        let proof_job_ids = self
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

    pub async fn get_leaf_aggregation_job_attempts(
        &mut self,
        id: u32,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                leaf_aggregation_witness_jobs_fri
            WHERE
                id = $1
            "#,
            i64::from(id)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.attempts as u32);

        Ok(attempts)
    }

    async fn prover_job_ids_for(
        &mut self,
        block_number: L1BatchNumber,
        circuit_id: u8,
        round: AggregationRound,
        depth: u16,
    ) -> Vec<u32> {
        sqlx::query!(
            r#"
            SELECT
                id
            FROM
                prover_jobs_fri
            WHERE
                l1_batch_number = $1
                AND circuit_id = $2
                AND aggregation_round = $3
                AND depth = $4
                AND status = 'successful'
            ORDER BY
                sequence_number ASC;
            "#,
            i64::from(block_number.0),
            i16::from(circuit_id),
            round as i16,
            i32::from(depth)
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.id as u32)
        .collect::<_>()
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
                            JOIN leaf_aggregation_witness_jobs_fri lawj ON prover_jobs_fri.l1_batch_number = lawj.l1_batch_number
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
        protocol_versions: &[FriProtocolVersionId],
        picked_by: &str,
    ) -> Option<NodeAggregationJobMetadata> {
        let protocol_versions: Vec<i32> = protocol_versions.iter().map(|&id| id as i32).collect();
        let row = sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $2
            WHERE
                id = (
                    SELECT
                        id
                    FROM
                        node_aggregation_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = ANY ($1)
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
            &protocol_versions[..],
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

    pub async fn get_node_aggregation_job_attempts(
        &mut self,
        id: u32,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                node_aggregation_witness_jobs_fri
            WHERE
                id = $1
            "#,
            i64::from(id)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.attempts as u32);

        Ok(attempts)
    }

    pub async fn mark_node_aggregation_job_failed(&mut self, error: &str, id: u32) {
        sqlx::query!(
            r#"
            UPDATE node_aggregation_witness_jobs_fri
            SET
                status = 'failed',
                error = $1,
                updated_at = NOW()
            WHERE
                id = $2
            "#,
            error,
            i64::from(id)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
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
        protocol_version_id: FriProtocolVersionId,
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
                    updated_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, 'waiting_for_proofs', NOW(), NOW())
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
            protocol_version_id as i32,
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
                            JOIN node_aggregation_witness_jobs_fri nawj ON prover_jobs_fri.l1_batch_number = nawj.l1_batch_number
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
                            JOIN node_aggregation_witness_jobs_fri nawj ON prover_jobs_fri.l1_batch_number = nawj.l1_batch_number
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

    pub async fn requeue_stuck_leaf_aggregations_jobs(
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
                attempts
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
        })
        .collect()
    }

    pub async fn requeue_stuck_node_aggregations_jobs(
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
                attempts
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
        })
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
                attempts
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
        })
        .collect()
    }

    pub async fn get_next_scheduler_witness_job(
        &mut self,
        protocol_versions: &[FriProtocolVersionId],
        picked_by: &str,
    ) -> Option<L1BatchNumber> {
        let protocol_versions: Vec<i32> = protocol_versions.iter().map(|&id| id as i32).collect();
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
                        AND protocol_version = ANY ($1)
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
            &protocol_versions[..],
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
    }

    pub async fn get_scheduler_witness_job_attempts(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                scheduler_witness_jobs_fri
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

    pub async fn mark_scheduler_job_failed(&mut self, error: &str, block_number: L1BatchNumber) {
        sqlx::query!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'failed',
                error = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            error,
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_witness_jobs_stats(
        &mut self,
        aggregation_round: AggregationRound,
    ) -> JobCountStatistics {
        let table_name = Self::input_table_name_for(aggregation_round);
        let sql = format!(
            r#"
                SELECT COUNT(*) as "count", status as "status"
                FROM {}
                GROUP BY status
                "#,
            table_name
        );
        let mut results: HashMap<String, i64> = sqlx::query(&sql)
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

    fn input_table_name_for(aggregation_round: AggregationRound) -> &'static str {
        match aggregation_round {
            AggregationRound::BasicCircuits => "witness_inputs_fri",
            AggregationRound::LeafAggregation => "leaf_aggregation_witness_jobs_fri",
            AggregationRound::NodeAggregation => "node_aggregation_witness_jobs_fri",
            AggregationRound::Scheduler => "scheduler_witness_jobs_fri",
        }
    }

    pub async fn protocol_version_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> FriProtocolVersionId {
        sqlx::query!(
            r#"
            SELECT
                protocol_version
            FROM
                witness_inputs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .protocol_version
        .map(|id| FriProtocolVersionId::try_from(id as u16).unwrap())
        .unwrap()
    }
}
