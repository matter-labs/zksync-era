#![doc = include_str!("../doc/FriWitnessGeneratorDal.md")]

use std::{collections::HashMap, str::FromStr, time::Duration};

use sqlx::{types::chrono::NaiveDateTime, Row};
use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    prover_dal::{
        BasicWitnessGeneratorJobInfo, JobCountStatistics, LeafAggregationJobMetadata,
        LeafWitnessGeneratorJobInfo, NodeAggregationJobMetadata, NodeWitnessGeneratorJobInfo,
        ProofGenerationTime, RecursionTipWitnessGeneratorJobInfo, SchedulerWitnessGeneratorJobInfo,
        StuckJobs, WitnessJobStatus,
    },
    L1BatchNumber,
};
use zksync_db_connection::{
    connection::Connection, metrics::MethodLatency, utils::naive_time_from_pg_interval,
};

use crate::{duration_to_naive_time, pg_interval_from_duration, Prover};

#[derive(Debug)]
pub struct FriWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
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
        witness_inputs_blob_url: &str,
        protocol_version: ProtocolSemanticVersion,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
                witness_inputs_fri (
                    l1_batch_number,
                    witness_inputs_blob_url,
                    protocol_version,
                    status,
                    created_at,
                    updated_at,
                    protocol_version_patch
                )
            VALUES
                ($1, $2, $3, 'queued', NOW(), NOW(), $4)
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(block_number.0),
            witness_inputs_blob_url,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
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
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<L1BatchNumber> {
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
                        AND protocol_version = $2
                        AND protocol_version_patch = $4
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                witness_inputs_fri.l1_batch_number
            "#,
            i64::from(last_l1_batch_to_process),
            protocol_version.minor as i32,
            picked_by,
            protocol_version.patch.0 as i32,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
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
                AND status != 'successful'
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
                AND status != 'successful'
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
                AND status != 'successful'
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
            circuit_id: None,
        })
        .collect()
    }

    /// Responsible for creating the jobs to be processed, after a basic witness generator run.
    /// It will create as follows:
    /// - all prover jobs for aggregation round 0 identified in the basic witness generator run
    /// - all leaf aggregation jobs for the batch
    /// - all node aggregation jobs at depth 0 for the batch
    /// - the recursion tip witness job
    /// - the scheduler witness job
    /// NOTE: Not all batches have all circuits, so it's possible we'll be missing some aggregation jobs (for circuits not present in the batch).
    pub async fn create_aggregation_jobs(
        &mut self,
        block_number: L1BatchNumber,
        closed_form_inputs_and_urls: &Vec<(u8, String, usize)>,
        scheduler_partial_input_blob_url: &str,
        base_layer_to_recursive_layer_circuit_id: fn(u8) -> u8,
        protocol_version: ProtocolSemanticVersion,
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
                    i16::from(*circuit_id),
                    closed_form_inputs_url,
                    *number_of_basic_circuits as i32,
                    protocol_version.minor as i32,
                    protocol_version.patch.0 as i32,
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
                    protocol_version,
                )
                .await;
            }

            sqlx::query!(
                r#"
                INSERT INTO
                    recursion_tip_witness_jobs_fri (
                        l1_batch_number,
                        status,
                        number_of_final_node_jobs,
                        protocol_version,
                        created_at,
                        updated_at,
                        protocol_version_patch
                    )
                VALUES
                    ($1, 'waiting_for_proofs', $2, $3, NOW(), NOW(), $4)
                ON CONFLICT (l1_batch_number) DO
                UPDATE
                SET
                    updated_at = NOW()
                "#,
                block_number.0 as i64,
                closed_form_inputs_and_urls.len() as i32,
                protocol_version.minor as i32,
                protocol_version.patch.0 as i32,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();

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

            drop(latency);
        }
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
                AND status != 'successful'
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

    pub async fn move_recursion_tip_jobs_from_waiting_to_queued(&mut self) -> Vec<u64> {
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'queued'
            WHERE
                l1_batch_number IN (
                    SELECT
                        prover_jobs_fri.l1_batch_number
                    FROM
                        prover_jobs_fri
                        JOIN recursion_tip_witness_jobs_fri rtwj ON prover_jobs_fri.l1_batch_number = rtwj.l1_batch_number
                    WHERE
                        rtwj.status = 'waiting_for_proofs'
                        AND prover_jobs_fri.status = 'successful'
                        AND prover_jobs_fri.aggregation_round = $1
                        AND prover_jobs_fri.is_node_final_proof = TRUE
                    GROUP BY
                        prover_jobs_fri.l1_batch_number,
                        rtwj.number_of_final_node_jobs
                    HAVING
                        COUNT(*) = rtwj.number_of_final_node_jobs
                )
            RETURNING
                l1_batch_number;
            "#,
            AggregationRound::NodeAggregation as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.l1_batch_number as u64))
        .collect()
    }

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
                        JOIN scheduler_witness_jobs_fri swj ON prover_jobs_fri.l1_batch_number = swj.l1_batch_number
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
                attempts,
                circuit_id
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
                attempts,
                circuit_id
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
        })
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
            circuit_id: None,
        })
        .collect()
    }

    pub async fn get_next_recursion_tip_witness_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<(L1BatchNumber, i32)> {
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
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        recursion_tip_witness_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $2
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                recursion_tip_witness_jobs_fri.l1_batch_number,
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
                L1BatchNumber(row.l1_batch_number as u32),
                row.number_of_final_node_jobs,
            )
        })
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
            circuit_id: None,
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

    pub async fn get_recursion_tip_witness_job_attempts(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                recursion_tip_witness_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            l1_batch_number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.attempts as u32);

        Ok(attempts)
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

    pub async fn mark_recursion_tip_job_as_successful(
        &mut self,
        l1_batch_number: L1BatchNumber,
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
                l1_batch_number = $2
            "#,
            duration_to_naive_time(time_taken),
            l1_batch_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
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

    pub async fn mark_recursion_tip_job_failed(
        &mut self,
        error: &str,
        l1_batch_number: L1BatchNumber,
    ) {
        sqlx::query!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'failed',
                error = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND status != 'successful'
            "#,
            error,
            l1_batch_number.0 as i64
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
                AND status != 'successful'
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
    ) -> HashMap<(AggregationRound, ProtocolSemanticVersion), JobCountStatistics> {
        let table_name = Self::input_table_name_for(aggregation_round);
        let sql = format!(
            r#"
                SELECT
                    protocol_version,
                    protocol_version_patch,
                    COUNT(*) FILTER (WHERE status = 'queued') as queued,
                    COUNT(*) FILTER (WHERE status = 'in_progress') as in_progress
                FROM
                    {}
                WHERE protocol_version IS NOT NULL
                GROUP BY
                    protocol_version,
                    protocol_version_patch
                "#,
            table_name,
        );
        sqlx::query(&sql)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                let protocol_semantic_version = ProtocolSemanticVersion::new(
                    ProtocolVersionId::try_from(row.get::<i32, &str>("protocol_version") as u16)
                        .unwrap(),
                    VersionPatch(row.get::<i32, &str>("protocol_version_patch") as u32),
                );
                let key = (aggregation_round, protocol_semantic_version);
                let value = JobCountStatistics {
                    queued: row.get::<i64, &str>("queued") as usize,
                    in_progress: row.get::<i64, &str>("in_progress") as usize,
                };
                (key, value)
            })
            .collect()
    }

    fn input_table_name_for(aggregation_round: AggregationRound) -> &'static str {
        match aggregation_round {
            AggregationRound::BasicCircuits => "witness_inputs_fri",
            AggregationRound::LeafAggregation => "leaf_aggregation_witness_jobs_fri",
            AggregationRound::NodeAggregation => "node_aggregation_witness_jobs_fri",
            AggregationRound::RecursionTip => "recursion_tip_witness_jobs_fri",
            AggregationRound::Scheduler => "scheduler_witness_jobs_fri",
        }
    }

    pub async fn protocol_version_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> ProtocolSemanticVersion {
        let result = sqlx::query!(
            r#"
            SELECT
                protocol_version,
                protocol_version_patch
            FROM
                witness_inputs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        ProtocolSemanticVersion::new(
            ProtocolVersionId::try_from(result.protocol_version.unwrap() as u16).unwrap(),
            VersionPatch(result.protocol_version_patch as u32),
        )
    }

    pub async fn get_basic_witness_generator_job_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<BasicWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                witness_inputs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| BasicWitnessGeneratorJobInfo {
            l1_batch_number,
            witness_inputs_blob_url: row.witness_inputs_blob_url,
            attempts: row.attempts as u32,
            status: row.status.parse::<WitnessJobStatus>().unwrap(),
            error: row.error,
            created_at: row.created_at,
            updated_at: row.updated_at,
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            protocol_version: row.protocol_version,
            picked_by: row.picked_by,
        })
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

    pub async fn get_recursion_tip_witness_generator_jobs_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<RecursionTipWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                recursion_tip_witness_jobs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| RecursionTipWitnessGeneratorJobInfo {
            l1_batch_number,
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

    pub async fn delete_witness_generator_data_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        aggregation_round: AggregationRound,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query(
            format!(
                r#"
            DELETE FROM
                {table}
            WHERE
                l1_batch_number = {l1_batch_number}
            "#,
                table = Self::input_table_name_for(aggregation_round),
                l1_batch_number = i64::from(block_number.0),
            )
            .as_str(),
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete_batch_data(
        &mut self,
        block_number: L1BatchNumber,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        self.delete_witness_generator_data_for_batch(block_number, AggregationRound::BasicCircuits)
            .await?;
        self.delete_witness_generator_data_for_batch(
            block_number,
            AggregationRound::LeafAggregation,
        )
        .await?;
        self.delete_witness_generator_data_for_batch(
            block_number,
            AggregationRound::NodeAggregation,
        )
        .await?;
        self.delete_witness_generator_data(AggregationRound::RecursionTip)
            .await?;
        self.delete_witness_generator_data_for_batch(block_number, AggregationRound::Scheduler)
            .await
    }

    pub async fn delete_witness_generator_data(
        &mut self,
        aggregation_round: AggregationRound,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query(
            format!(
                r#"
            DELETE FROM
                {}
            "#,
                Self::input_table_name_for(aggregation_round)
            )
            .as_str(),
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete(&mut self) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        self.delete_witness_generator_data(AggregationRound::BasicCircuits)
            .await?;
        self.delete_witness_generator_data(AggregationRound::LeafAggregation)
            .await?;
        self.delete_witness_generator_data(AggregationRound::NodeAggregation)
            .await?;
        self.delete_witness_generator_data(AggregationRound::RecursionTip)
            .await?;
        self.delete_witness_generator_data(AggregationRound::Scheduler)
            .await
    }

    pub async fn requeue_stuck_witness_inputs_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let query = format!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = {}
                AND attempts >= {}
                AND (status = 'in_progress' OR status = 'failed')
            RETURNING
                l1_batch_number,
                status,
                attempts
            "#,
            i64::from(block_number.0),
            max_attempts
        );
        sqlx::query(&query)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: row.get::<i64, &str>("l1_batch_number") as u64,
                status: row.get("status"),
                attempts: row.get::<i16, &str>("attempts") as u64,
                circuit_id: None,
            })
            .collect()
    }

    pub async fn requeue_stuck_leaf_aggregation_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        self.requeue_stuck_jobs_for_batch_in_aggregation_round(
            AggregationRound::LeafAggregation,
            block_number,
            max_attempts,
        )
        .await
    }

    pub async fn requeue_stuck_node_aggregation_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        self.requeue_stuck_jobs_for_batch_in_aggregation_round(
            AggregationRound::NodeAggregation,
            block_number,
            max_attempts,
        )
        .await
    }

    pub async fn requeue_stuck_recursion_tip_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let query = format!(
            r#"
            UPDATE recursion_tip_witness_jobs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = {}
                AND attempts >= {}
                AND (status = 'in_progress' OR status = 'failed')
            RETURNING
                l1_batch_number,
                status,
                attempts
            "#,
            i64::from(block_number.0),
            max_attempts
        );
        sqlx::query(&query)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: row.get::<i64, &str>("l1_batch_number") as u64,
                status: row.get("status"),
                attempts: row.get::<i16, &str>("attempts") as u64,
                circuit_id: None,
            })
            .collect()
    }

    pub async fn requeue_stuck_scheduler_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let query = format!(
            r#"
            UPDATE scheduler_witness_jobs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = {}
                AND attempts >= {}
                AND (status = 'in_progress' OR status = 'failed')
            RETURNING
                l1_batch_number,
                status,
                attempts
            "#,
            i64::from(block_number.0),
            max_attempts
        );
        sqlx::query(&query)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: row.get::<i64, &str>("l1_batch_number") as u64,
                status: row.get("status"),
                attempts: row.get::<i16, &str>("attempts") as u64,
                circuit_id: None,
            })
            .collect()
    }

    async fn requeue_stuck_jobs_for_batch_in_aggregation_round(
        &mut self,
        aggregation_round: AggregationRound,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let table_name = Self::input_table_name_for(aggregation_round);
        let job_id_table_name = Self::job_id_table_name_for(aggregation_round);
        let query = format!(
            r#"
            UPDATE {}
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = {}
                AND attempts >= {}
                AND (status = 'in_progress' OR status = 'failed')
            RETURNING
                {},
                status,
                attempts,
                circuit_id
            "#,
            table_name,
            i64::from(block_number.0),
            max_attempts,
            job_id_table_name
        );
        sqlx::query(&query)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: row.get::<i64, &str>(job_id_table_name) as u64,
                status: row.get("status"),
                attempts: row.get::<i16, &str>("attempts") as u64,
                circuit_id: Some(row.get::<i16, &str>("circuit_id") as u32),
            })
            .collect()
    }

    fn job_id_table_name_for(aggregation_round: AggregationRound) -> &'static str {
        match aggregation_round {
            AggregationRound::BasicCircuits
            | AggregationRound::RecursionTip
            | AggregationRound::Scheduler => "l1_batch_number",
            AggregationRound::LeafAggregation | AggregationRound::NodeAggregation => "id",
        }
    }

    pub async fn get_proof_generation_times_for_time_frame(
        &mut self,
        time_frame: NaiveDateTime,
    ) -> sqlx::Result<Vec<ProofGenerationTime>> {
        let proof_generation_times = sqlx::query!(
            r#"
            SELECT
                comp.l1_batch_number,
                (comp.updated_at - wit.created_at) AS time_taken,
                wit.created_at
            FROM
                proof_compression_jobs_fri AS comp
                JOIN witness_inputs_fri AS wit ON comp.l1_batch_number = wit.l1_batch_number
            WHERE
                wit.created_at > $1
            ORDER BY
                time_taken DESC;
            "#,
            time_frame.into(),
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| ProofGenerationTime {
            l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
            time_taken: naive_time_from_pg_interval(
                row.time_taken.expect("time_taken must be present"),
            ),
            created_at: row.created_at,
        })
        .collect();
        Ok(proof_generation_times)
    }
}
