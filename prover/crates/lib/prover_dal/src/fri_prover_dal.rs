#![doc = include_str!("../doc/FriProverDal.md")]
use std::{collections::HashMap, convert::TryFrom, str::FromStr, time::Duration};

use zksync_basic_types::{
    basic_fri_types::{AggregationRound, CircuitIdRoundTuple, JobIdentifiers},
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
    prover_dal::{
        FriProverJobMetadata, JobCountStatistics, ProverJobFriInfo, ProverJobStatus, StuckJobs,
    },
    L1BatchNumber,
};
use zksync_db_connection::{
    connection::Connection, instrument::InstrumentExt, metrics::MethodLatency,
};

use crate::{duration_to_naive_time, pg_interval_from_duration, Prover};

#[derive(Debug)]
pub struct FriProverDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriProverDal<'_, '_> {
    pub async fn insert_prover_jobs(
        &mut self,
        l1_batch_number: L1BatchNumber,
        circuit_ids_and_urls: Vec<(u8, String)>,
        aggregation_round: AggregationRound,
        depth: u16,
        protocol_version_id: ProtocolSemanticVersion,
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
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<FriProverJobMetadata> {
        sqlx::query!(
            r#"
            UPDATE prover_jobs_fri
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
                        prover_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $2
                    ORDER BY
                        aggregation_round DESC,
                        l1_batch_number ASC,
                        id ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                prover_jobs_fri.id,
                prover_jobs_fri.l1_batch_number,
                prover_jobs_fri.circuit_id,
                prover_jobs_fri.aggregation_round,
                prover_jobs_fri.sequence_number,
                prover_jobs_fri.depth,
                prover_jobs_fri.is_node_final_proof
            "#,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| FriProverJobMetadata {
            id: row.id as u32,
            block_number: L1BatchNumber(row.l1_batch_number as u32),
            circuit_id: row.circuit_id as u8,
            aggregation_round: AggregationRound::try_from(i32::from(row.aggregation_round))
                .unwrap(),
            sequence_number: row.sequence_number as usize,
            depth: row.depth as u16,
            is_node_final_proof: row.is_node_final_proof,
        })
    }

    pub async fn get_next_job_for_circuit_id_round(
        &mut self,
        circuits_to_pick: &[CircuitIdRoundTuple],
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<FriProverJobMetadata> {
        let circuit_ids: Vec<_> = circuits_to_pick
            .iter()
            .map(|tuple| i16::from(tuple.circuit_id))
            .collect();
        let aggregation_rounds: Vec<_> = circuits_to_pick
            .iter()
            .map(|tuple| i16::from(tuple.aggregation_round))
            .collect();
        sqlx::query!(
            r#"
            UPDATE prover_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                processing_started_at = NOW(),
                updated_at = NOW(),
                picked_by = $5
            WHERE
                id = (
                    SELECT
                        pj.id
                    FROM
                        (
                            SELECT
                                *
                            FROM
                                UNNEST($1::SMALLINT[], $2::SMALLINT[])
                        ) AS tuple (circuit_id, ROUND)
                        JOIN LATERAL (
                            SELECT
                                *
                            FROM
                                prover_jobs_fri AS pj
                            WHERE
                                pj.status = 'queued'
                                AND pj.protocol_version = $3
                                AND pj.protocol_version_patch = $4
                                AND pj.circuit_id = tuple.circuit_id
                                AND pj.aggregation_round = tuple.round
                            ORDER BY
                                pj.l1_batch_number ASC,
                                pj.id ASC
                            LIMIT
                                1
                        ) AS pj ON TRUE
                    ORDER BY
                        pj.l1_batch_number ASC,
                        pj.aggregation_round DESC,
                        pj.id ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                prover_jobs_fri.id,
                prover_jobs_fri.l1_batch_number,
                prover_jobs_fri.circuit_id,
                prover_jobs_fri.aggregation_round,
                prover_jobs_fri.sequence_number,
                prover_jobs_fri.depth,
                prover_jobs_fri.is_node_final_proof
            "#,
            &circuit_ids[..],
            &aggregation_rounds[..],
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
            picked_by,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| FriProverJobMetadata {
            id: row.id as u32,
            block_number: L1BatchNumber(row.l1_batch_number as u32),
            circuit_id: row.circuit_id as u8,
            aggregation_round: AggregationRound::try_from(i32::from(row.aggregation_round))
                .unwrap(),
            sequence_number: row.sequence_number as usize,
            depth: row.depth as u16,
            is_node_final_proof: row.is_node_final_proof,
        })
    }

    pub async fn save_proof_error(&mut self, id: u32, error: String) {
        {
            sqlx::query!(
                r#"
                UPDATE prover_jobs_fri
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
    }

    pub async fn get_prover_job_attempts(&mut self, id: u32) -> sqlx::Result<Option<u32>> {
        let attempts = sqlx::query!(
            r#"
            SELECT
                attempts
            FROM
                prover_jobs_fri
            WHERE
                id = $1
            "#,
            i64::from(id)
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
            r#"
            UPDATE prover_jobs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1,
                proof_blob_url = $2
            WHERE
                id = $3
            RETURNING
                prover_jobs_fri.id,
                prover_jobs_fri.l1_batch_number,
                prover_jobs_fri.circuit_id,
                prover_jobs_fri.aggregation_round,
                prover_jobs_fri.sequence_number,
                prover_jobs_fri.depth,
                prover_jobs_fri.is_node_final_proof
            "#,
            duration_to_naive_time(time_taken),
            blob_url,
            i64::from(id)
        )
        .instrument("save_fri_proof")
        .report_latency()
        .with_arg("id", &id)
        .fetch_optional(self.storage)
        .await
        .unwrap()
        .map(|row| FriProverJobMetadata {
            id: row.id as u32,
            block_number: L1BatchNumber(row.l1_batch_number as u32),
            circuit_id: row.circuit_id as u8,
            aggregation_round: AggregationRound::try_from(i32::from(row.aggregation_round))
                .unwrap(),
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
                r#"
                UPDATE prover_jobs_fri
                SET
                    status = 'queued',
                    updated_at = NOW(),
                    processing_started_at = NOW()
                WHERE
                    id IN (
                        SELECT
                            id
                        FROM
                            prover_jobs_fri
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
                        FOR UPDATE
                            SKIP LOCKED
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
        protocol_version: ProtocolSemanticVersion,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
                prover_jobs_fri (
                    l1_batch_number,
                    circuit_id,
                    circuit_blob_url,
                    aggregation_round,
                    sequence_number,
                    depth,
                    is_node_final_proof,
                    protocol_version,
                    status,
                    created_at,
                    updated_at,
                    protocol_version_patch
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, 'queued', NOW(), NOW(), $9)
            ON CONFLICT (l1_batch_number, aggregation_round, circuit_id, depth, sequence_number) DO
            UPDATE
            SET
                updated_at = NOW()
            "#,
            i64::from(l1_batch_number.0),
            i16::from(circuit_id),
            circuit_blob_url,
            aggregation_round as i64,
            sequence_number as i64,
            i32::from(depth),
            is_node_final_proof,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_prover_jobs_stats(&mut self) -> HashMap<JobIdentifiers, JobCountStatistics> {
        {
            let rows = sqlx::query!(
                r#"
                SELECT
                    COUNT(*) AS "count!",
                    circuit_id AS "circuit_id!",
                    aggregation_round AS "aggregation_round!",
                    status AS "status!",
                    protocol_version AS "protocol_version!",
                    protocol_version_patch AS "protocol_version_patch!"
                FROM
                    prover_jobs_fri
                WHERE
                    (
                        status = 'queued'
                        OR status = 'in_progress'
                    )
                    AND protocol_version IS NOT NULL
                GROUP BY
                    circuit_id,
                    aggregation_round,
                    status,
                    protocol_version,
                    protocol_version_patch
                "#
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();

            let mut result = HashMap::new();

            for row in &rows {
                let stats: &mut JobCountStatistics = result
                    .entry(JobIdentifiers {
                        circuit_id: row.circuit_id as u8,
                        aggregation_round: row.aggregation_round as u8,
                        protocol_version: row.protocol_version as u16,
                        protocol_version_patch: row.protocol_version_patch as u32,
                    })
                    .or_default();
                match row.status.as_ref() {
                    "queued" => stats.queued = row.count as usize,
                    "in_progress" => stats.in_progress = row.count as usize,
                    _ => (),
                }
            }

            result
        }
    }

    pub async fn min_unproved_l1_batch_number(&mut self) -> HashMap<(u8, u8), L1BatchNumber> {
        {
            sqlx::query!(
                r#"
                SELECT
                    MIN(l1_batch_number) AS "l1_batch_number!",
                    circuit_id,
                    aggregation_round
                FROM
                    prover_jobs_fri
                WHERE
                    status IN ('queued', 'in_gpu_proof', 'in_progress', 'failed')
                GROUP BY
                    circuit_id,
                    aggregation_round
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

    pub async fn min_unproved_l1_batch_number_for_aggregation_round(
        &mut self,
        aggregation_round: AggregationRound,
    ) -> Option<L1BatchNumber> {
        sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                prover_jobs_fri
            WHERE
                status <> 'skipped'
                AND status <> 'successful'
                AND aggregation_round = $1
            ORDER BY
                l1_batch_number ASC
            LIMIT
                1
            "#,
            aggregation_round as i16
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
    }

    pub async fn update_status(&mut self, id: u32, status: &str) {
        sqlx::query!(
            r#"
            UPDATE prover_jobs_fri
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                id = $2
                AND status != 'successful'
            "#,
            status,
            i64::from(id)
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
            r#"
            SELECT
                id
            FROM
                prover_jobs_fri
            WHERE
                l1_batch_number = $1
                AND status = 'successful'
                AND aggregation_round = $2
            "#,
            i64::from(l1_batch_number.0),
            AggregationRound::Scheduler as i16,
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?
        .map(|row| row.id as u32)
    }

    pub async fn get_recursion_tip_proof_job_id(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<u32> {
        sqlx::query!(
            r#"
            SELECT
                id
            FROM
                prover_jobs_fri
            WHERE
                l1_batch_number = $1
                AND status = 'successful'
                AND aggregation_round = $2
            "#,
            l1_batch_number.0 as i64,
            AggregationRound::RecursionTip as i16,
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?
        .map(|row| row.id as u32)
    }

    pub async fn archive_old_jobs(&mut self, archiving_interval_secs: u64) -> usize {
        let archiving_interval_secs =
            pg_interval_from_duration(Duration::from_secs(archiving_interval_secs));

        sqlx::query_scalar!(
            r#"
            WITH deleted AS (
                DELETE FROM prover_jobs_fri AS p
                USING proof_compression_jobs_fri AS c
                WHERE
                    p.status NOT IN ('queued', 'in_progress', 'in_gpu_proof', 'failed')
                    AND p.updated_at < NOW() - $1::INTERVAL
                    AND p.l1_batch_number = c.l1_batch_number
                    AND c.status = 'sent_to_server'
                RETURNING p.*
            ),
            inserted_count AS (
                INSERT INTO prover_jobs_fri_archive
                SELECT * FROM deleted
            )
            SELECT COUNT(*) FROM deleted
            "#,
            &archiving_interval_secs,
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .unwrap_or(0) as usize
    }

    pub async fn get_final_node_proof_job_ids_for(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Vec<(u8, u32)> {
        sqlx::query!(
            r#"
            SELECT
                circuit_id,
                id
            FROM
                prover_jobs_fri
            WHERE
                l1_batch_number = $1
                AND is_node_final_proof = TRUE
                AND status = 'successful'
            ORDER BY
                circuit_id ASC
            "#,
            l1_batch_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.circuit_id as u8, row.id as u32))
        .collect()
    }

    pub async fn get_prover_jobs_stats_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
        aggregation_round: AggregationRound,
    ) -> Vec<ProverJobFriInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                prover_jobs_fri
            WHERE
                l1_batch_number = $1
                AND aggregation_round = $2
            "#,
            i64::from(l1_batch_number.0),
            aggregation_round as i16
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .iter()
        .map(|row| ProverJobFriInfo {
            id: row.id as u32,
            l1_batch_number,
            circuit_id: row.circuit_id as u32,
            circuit_blob_url: row.circuit_blob_url.clone(),
            aggregation_round,
            sequence_number: row.sequence_number as u32,
            status: ProverJobStatus::from_str(&row.status).unwrap(),
            error: row.error.clone(),
            attempts: row.attempts as u8,
            processing_started_at: row.processing_started_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
            time_taken: row.time_taken,
            depth: row.depth as u32,
            is_node_final_proof: row.is_node_final_proof,
            proof_blob_url: row.proof_blob_url.clone(),
            protocol_version: row.protocol_version.map(|protocol_version| {
                ProtocolVersionId::try_from(protocol_version as u16).unwrap()
            }),
            picked_by: row.picked_by.clone(),
        })
        .collect()
    }

    pub async fn delete_prover_jobs_fri_batch_data(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM prover_jobs_fri
            WHERE
                l1_batch_number = $1;
            "#,
            i64::from(l1_batch_number.0)
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete_batch_data(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        self.delete_prover_jobs_fri_batch_data(l1_batch_number)
            .await
    }

    pub async fn delete_prover_jobs_fri(&mut self) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM prover_jobs_fri
            "#
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete(&mut self) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        self.delete_prover_jobs_fri().await
    }

    pub async fn requeue_stuck_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        {
            sqlx::query!(
                r#"
                UPDATE prover_jobs_fri
                SET
                    status = 'queued',
                    error = 'Manually requeued',
                    attempts = 2,
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
                    id,
                    status,
                    attempts,
                    circuit_id
                "#,
                i64::from(block_number.0),
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
    }
}
