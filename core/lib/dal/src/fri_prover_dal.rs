use std::{collections::HashMap, convert::TryFrom, time::Duration};

use zksync_types::{
    basic_fri_types::CircuitIdRoundTuple,
    proofs::{AggregationRound, FriProverJobMetadata, JobCountStatistics, StuckJobs},
    protocol_version::FriProtocolVersionId,
    L1BatchNumber,
};

use crate::{
    instrument::InstrumentExt,
    metrics::MethodLatency,
    time_utils::{duration_to_naive_time, pg_interval_from_duration},
    StorageProcessor,
};

#[derive(Debug)]
pub struct FriProverDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl FriProverDal<'_, '_> {
    pub async fn insert_prover_jobs(
        &mut self,
        l1_batch_number: L1BatchNumber,
        circuit_ids_and_urls: Vec<(u8, String)>,
        aggregation_round: AggregationRound,
        depth: u16,
        protocol_version_id: FriProtocolVersionId,
    ) {
        let latency = MethodLatency::new("save_fri_prover_jobs");
        for (sequence_number, (circuit_id, circuit_blob_url)) in
            circuit_ids_and_urls.iter().enumerate()
        {
            self.insert_prover_job(
                l1_batch_number,
                *circuit_id,
                depth,
                sequence_number,
                aggregation_round,
                circuit_blob_url,
                false,
                protocol_version_id,
            )
            .await;
        }
        drop(latency);
    }

    pub async fn get_next_job(
        &mut self,
        protocol_versions: &[FriProtocolVersionId],
        picked_by: &str,
    ) -> Option<FriProverJobMetadata> {
        let protocol_versions: Vec<i32> = protocol_versions.iter().map(|&id| id as i32).collect();
        sqlx::query!(
            "
                UPDATE prover_jobs_fri
                SET status = 'in_progress', attempts = attempts + 1,
                    updated_at = now(), processing_started_at = now(),
                    picked_by = $2
                WHERE id = (
                    SELECT id
                    FROM prover_jobs_fri
                    WHERE status = 'queued'
                    AND protocol_version = ANY($1)
                    ORDER BY aggregation_round DESC, l1_batch_number ASC, id ASC
                    LIMIT 1
                    FOR UPDATE
                    SKIP LOCKED
                )
                RETURNING prover_jobs_fri.id, prover_jobs_fri.l1_batch_number, prover_jobs_fri.circuit_id,
                prover_jobs_fri.aggregation_round, prover_jobs_fri.sequence_number, prover_jobs_fri.depth,
                prover_jobs_fri.is_node_final_proof
                ",
            &protocol_versions[..],
            picked_by,
        )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|row| FriProverJobMetadata {
                id: row.id as u32,
                block_number: L1BatchNumber(row.l1_batch_number as u32),
                circuit_id: row.circuit_id as u8,
                aggregation_round: AggregationRound::try_from(row.aggregation_round as i32).unwrap(),
                sequence_number: row.sequence_number as usize,
                depth: row.depth as u16,
                is_node_final_proof: row.is_node_final_proof,
            })
    }

    pub async fn get_next_job_for_circuit_id_round(
        &mut self,
        circuits_to_pick: &[CircuitIdRoundTuple],
        protocol_versions: &[FriProtocolVersionId],
        picked_by: &str,
    ) -> Option<FriProverJobMetadata> {
        let circuit_ids: Vec<_> = circuits_to_pick
            .iter()
            .map(|tuple| tuple.circuit_id as i16)
            .collect();
        let protocol_versions: Vec<i32> = protocol_versions.iter().map(|&id| id as i32).collect();
        let aggregation_rounds: Vec<_> = circuits_to_pick
            .iter()
            .map(|tuple| tuple.aggregation_round as i16)
            .collect();
        sqlx::query!(
            "
                UPDATE prover_jobs_fri
                SET status = 'in_progress', attempts = attempts + 1,
                    updated_at = now(), processing_started_at = now(),
                    picked_by = $4
                WHERE id = (
                    SELECT pj.id
                    FROM prover_jobs_fri AS pj
                    JOIN (
                        SELECT * FROM unnest($1::smallint[], $2::smallint[])
                    )
                    AS tuple (circuit_id, round)
                    ON tuple.circuit_id = pj.circuit_id AND tuple.round = pj.aggregation_round
                    WHERE pj.status = 'queued'
                    AND pj.protocol_version = ANY($3)
                    ORDER BY pj.l1_batch_number ASC, pj.aggregation_round DESC, pj.id ASC
                    LIMIT 1
                    FOR UPDATE
                    SKIP LOCKED
                )
                RETURNING prover_jobs_fri.id, prover_jobs_fri.l1_batch_number, prover_jobs_fri.circuit_id,
                prover_jobs_fri.aggregation_round, prover_jobs_fri.sequence_number, prover_jobs_fri.depth,
                prover_jobs_fri.is_node_final_proof
                ",
            &circuit_ids[..],
            &aggregation_rounds[..],
            &protocol_versions[..],
            picked_by,
        )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|row| FriProverJobMetadata {
                id: row.id as u32,
                block_number: L1BatchNumber(row.l1_batch_number as u32),
                circuit_id: row.circuit_id as u8,
                aggregation_round: AggregationRound::try_from(row.aggregation_round as i32).unwrap(),
                sequence_number: row.sequence_number as usize,
                depth: row.depth as u16,
                is_node_final_proof: row.is_node_final_proof,
            })
    }

    pub async fn save_proof_error(&mut self, id: u32, error: String) {
        {
            sqlx::query!(
                "
                UPDATE prover_jobs_fri
                SET status = 'failed', error = $1, updated_at = now()
                WHERE id = $2
                ",
                error,
                id as i64,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn get_prover_job_attempts(&mut self, id: u32) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            "SELECT attempts FROM prover_jobs_fri WHERE id = $1",
            id as i64,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.attempts as u32);

        Ok(attempts)
    }

    pub async fn save_proof(
        &mut self,
        id: u32,
        time_taken: Duration,
        blob_url: &str,
    ) -> FriProverJobMetadata {
        sqlx::query!(
                "
                UPDATE prover_jobs_fri
                SET status = 'successful', updated_at = now(), time_taken = $1, proof_blob_url=$2
                WHERE id = $3
                RETURNING prover_jobs_fri.id, prover_jobs_fri.l1_batch_number, prover_jobs_fri.circuit_id,
                prover_jobs_fri.aggregation_round, prover_jobs_fri.sequence_number, prover_jobs_fri.depth,
                prover_jobs_fri.is_node_final_proof
                ",
            duration_to_naive_time(time_taken),
            blob_url,
            id as i64,
        )
            .instrument("save_fri_proof")
            .report_latency()
            .with_arg("id", &id)
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|row| FriProverJobMetadata {
                id: row.id as u32,
                block_number: L1BatchNumber(row.l1_batch_number as u32),
                circuit_id: row.circuit_id as u8,
                aggregation_round: AggregationRound::try_from(row.aggregation_round as i32).unwrap(),
                sequence_number: row.sequence_number as usize,
                depth: row.depth as u16,
                is_node_final_proof: row.is_node_final_proof,
            })
            .unwrap()
    }

    pub async fn requeue_stuck_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        {
            sqlx::query!(
                "
                UPDATE prover_jobs_fri
                SET status = 'queued', updated_at = now(), processing_started_at = now()
                WHERE id in (
                    SELECT id
                    FROM prover_jobs_fri
                    WHERE (status = 'in_progress' AND  processing_started_at <= now() - $1::interval AND attempts < $2)
                    OR (status = 'in_gpu_proof' AND  processing_started_at <= now() - $1::interval AND attempts < $2)
                    OR (status = 'failed' AND attempts < $2)
                    FOR UPDATE SKIP LOCKED
                )
                RETURNING id, status, attempts
                ",
                &processing_timeout,
                max_attempts as i32,
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .map(|row| StuckJobs { id: row.id as u64, status: row.status, attempts: row.attempts as u64 })
                .collect()
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_prover_job(
        &mut self,
        l1_batch_number: L1BatchNumber,
        circuit_id: u8,
        depth: u16,
        sequence_number: usize,
        aggregation_round: AggregationRound,
        circuit_blob_url: &str,
        is_node_final_proof: bool,
        protocol_version_id: FriProtocolVersionId,
    ) {
        sqlx::query!(
                    "
                    INSERT INTO prover_jobs_fri (l1_batch_number, circuit_id, circuit_blob_url, aggregation_round, sequence_number, depth, is_node_final_proof, protocol_version, status, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'queued', now(), now())
                    ON CONFLICT(l1_batch_number, aggregation_round, circuit_id, depth, sequence_number)
                    DO UPDATE SET updated_at=now()
                    ",
            l1_batch_number.0 as i64,
            circuit_id as i16,
            circuit_blob_url,
            aggregation_round as i64,
            sequence_number as i64,
            depth as i32,
            is_node_final_proof,
            protocol_version_id as i32,
        )
            .execute(self.storage.conn())
            .await
            .unwrap();
    }

    pub async fn get_prover_jobs_stats(&mut self) -> HashMap<(u8, u8), JobCountStatistics> {
        {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!", circuit_id as "circuit_id!", aggregation_round as "aggregation_round!", status as "status!"
                FROM prover_jobs_fri
                WHERE status <> 'skipped' and status <> 'successful'
                GROUP BY circuit_id, aggregation_round, status
                "#
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .map(|row| (row.circuit_id, row.aggregation_round, row.status, row.count as usize))
                .fold(HashMap::new(), |mut acc, (circuit_id, aggregation_round, status, value)| {
                    let stats = acc.entry((circuit_id as u8, aggregation_round as u8)).or_insert(JobCountStatistics {
                        queued: 0,
                        in_progress: 0,
                        failed: 0,
                        successful: 0,
                    });
                    match status.as_ref() {
                        "queued" => stats.queued = value,
                        "in_progress" => stats.in_progress = value,
                        "failed" => stats.failed = value,
                        "successful" => stats.successful = value,
                        _ => (),
                    }
                    acc
                })
        }
    }

    pub async fn min_unproved_l1_batch_number(&mut self) -> HashMap<(u8, u8), L1BatchNumber> {
        {
            sqlx::query!(
                r#"
                    SELECT MIN(l1_batch_number) as "l1_batch_number!", circuit_id, aggregation_round
                    FROM prover_jobs_fri
                    WHERE status IN('queued', 'in_gpu_proof', 'in_progress', 'failed')
                    GROUP BY circuit_id, aggregation_round
                "#
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    (row.circuit_id as u8, row.aggregation_round as u8),
                    L1BatchNumber(row.l1_batch_number as u32),
                )
            })
            .collect()
        }
    }

    pub async fn update_status(&mut self, id: u32, status: &str) {
        sqlx::query!(
            "UPDATE prover_jobs_fri \
                SET status = $1, updated_at = now() \
                WHERE id = $2",
            status,
            id as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn save_successful_sent_proof(&mut self, l1_batch_number: L1BatchNumber) {
        sqlx::query!(
            "UPDATE prover_jobs_fri \
                SET status = 'sent_to_server', updated_at = now() \
                WHERE l1_batch_number = $1",
            l1_batch_number.0 as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_scheduler_proof_job_id(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<u32> {
        sqlx::query!(
            "SELECT id from prover_jobs_fri \
             WHERE l1_batch_number = $1 \
             AND status = 'successful' \
             AND aggregation_round = $2",
            l1_batch_number.0 as i64,
            AggregationRound::Scheduler as i16,
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?
        .map(|row| row.id as u32)
    }
}
