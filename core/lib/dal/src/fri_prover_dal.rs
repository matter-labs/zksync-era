use std::{collections::HashMap, convert::TryFrom, time::Duration};

use zksync_types::{
    basic_fri_types::{AggregationRound, CircuitIdRoundTuple},
    protocol_version::FriProtocolVersionId,
    L1BatchNumber,
};

use self::types::{FriProverJobMetadata, JobCountStatistics, StuckJobs};
use crate::{
    instrument::InstrumentExt,
    metrics::MethodLatency,
    time_utils::{duration_to_naive_time, pg_interval_from_duration},
    StorageProcessor,
};

// TODO (PLA-775): Should not be an embedded submodule in a concrete DAL file.
pub mod types {
    //! Types exposed by the prover DAL for general-purpose use.

    use std::{net::IpAddr, ops::Add};

    use sqlx::types::chrono::{DateTime, Utc};
    use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

    // This currently lives in `zksync_prover_types` -- we don't want a dependency between prover types (`zkevm_test_harness`) and DAL.
    // This will be gone as part of 1.5.0, when EIP4844 becomes normal jobs, rather than special cased ones.
    pub(crate) const EIP_4844_CIRCUIT_ID: u8 = 255;

    #[derive(Debug, Clone)]
    pub struct FriProverJobMetadata {
        pub id: u32,
        pub block_number: L1BatchNumber,
        pub circuit_id: u8,
        pub aggregation_round: AggregationRound,
        pub sequence_number: usize,
        pub depth: u16,
        pub is_node_final_proof: bool,
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub struct JobCountStatistics {
        pub queued: usize,
        pub in_progress: usize,
        pub failed: usize,
        pub successful: usize,
    }

    impl Add for JobCountStatistics {
        type Output = JobCountStatistics;

        fn add(self, rhs: Self) -> Self::Output {
            Self {
                queued: self.queued + rhs.queued,
                in_progress: self.in_progress + rhs.in_progress,
                failed: self.failed + rhs.failed,
                successful: self.successful + rhs.successful,
            }
        }
    }

    #[derive(Debug)]
    pub struct StuckJobs {
        pub id: u64,
        pub status: String,
        pub attempts: u64,
    }

    // TODO (PLA-774): Redundant structure, should be replaced with `std::net::SocketAddr`.
    #[derive(Debug, Clone)]
    pub struct SocketAddress {
        pub host: IpAddr,
        pub port: u16,
    }

    impl From<SocketAddress> for std::net::SocketAddr {
        fn from(socket_address: SocketAddress) -> Self {
            Self::new(socket_address.host, socket_address.port)
        }
    }

    impl From<std::net::SocketAddr> for SocketAddress {
        fn from(socket_address: std::net::SocketAddr) -> Self {
            Self {
                host: socket_address.ip(),
                port: socket_address.port(),
            }
        }
    }

    #[derive(Debug, Clone)]
    pub struct LeafAggregationJobMetadata {
        pub id: u32,
        pub block_number: L1BatchNumber,
        pub circuit_id: u8,
        pub prover_job_ids_for_proofs: Vec<u32>,
    }

    #[derive(Debug, Clone)]
    pub struct NodeAggregationJobMetadata {
        pub id: u32,
        pub block_number: L1BatchNumber,
        pub circuit_id: u8,
        pub depth: u16,
        pub prover_job_ids_for_proofs: Vec<u32>,
    }

    #[derive(Debug)]
    pub struct JobPosition {
        pub aggregation_round: AggregationRound,
        pub sequence_number: usize,
    }

    #[derive(Debug, Default)]
    pub struct ProverJobStatusFailed {
        pub started_at: DateTime<Utc>,
        pub error: String,
    }

    #[derive(Debug)]
    pub struct ProverJobStatusSuccessful {
        pub started_at: DateTime<Utc>,
        pub time_taken: chrono::Duration,
    }

    impl Default for ProverJobStatusSuccessful {
        fn default() -> Self {
            ProverJobStatusSuccessful {
                started_at: DateTime::default(),
                time_taken: chrono::Duration::zero(),
            }
        }
    }

    #[derive(Debug, Default)]
    pub struct ProverJobStatusInProgress {
        pub started_at: DateTime<Utc>,
    }

    #[derive(Debug)]
    pub struct WitnessJobStatusSuccessful {
        pub started_at: DateTime<Utc>,
        pub time_taken: chrono::Duration,
    }

    impl Default for WitnessJobStatusSuccessful {
        fn default() -> Self {
            WitnessJobStatusSuccessful {
                started_at: DateTime::default(),
                time_taken: chrono::Duration::zero(),
            }
        }
    }

    #[derive(Debug, Default)]
    pub struct WitnessJobStatusFailed {
        pub started_at: DateTime<Utc>,
        pub error: String,
    }

    #[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
    pub enum ProverJobStatus {
        #[strum(serialize = "queued")]
        Queued,
        #[strum(serialize = "in_progress")]
        InProgress(ProverJobStatusInProgress),
        #[strum(serialize = "successful")]
        Successful(ProverJobStatusSuccessful),
        #[strum(serialize = "failed")]
        Failed(ProverJobStatusFailed),
        #[strum(serialize = "skipped")]
        Skipped,
        #[strum(serialize = "ignored")]
        Ignored,
    }

    #[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
    pub enum WitnessJobStatus {
        #[strum(serialize = "failed")]
        Failed(WitnessJobStatusFailed),
        #[strum(serialize = "skipped")]
        Skipped,
        #[strum(serialize = "successful")]
        Successful(WitnessJobStatusSuccessful),
        #[strum(serialize = "waiting_for_artifacts")]
        WaitingForArtifacts,
        #[strum(serialize = "waiting_for_proofs")]
        WaitingForProofs,
        #[strum(serialize = "in_progress")]
        InProgress,
        #[strum(serialize = "queued")]
        Queued,
    }

    #[derive(Debug)]
    pub struct WitnessJobInfo {
        pub block_number: L1BatchNumber,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        pub status: WitnessJobStatus,
        pub position: JobPosition,
    }

    #[derive(Debug)]
    pub struct ProverJobInfo {
        pub id: u32,
        pub block_number: L1BatchNumber,
        pub circuit_type: String,
        pub position: JobPosition,
        pub input_length: u64,
        pub status: ProverJobStatus,
        pub attempts: u32,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Debug)]
    pub struct JobExtendedStatistics {
        pub successful_padding: L1BatchNumber,
        pub queued_padding: L1BatchNumber,
        pub queued_padding_len: u32,
        pub active_area: Vec<ProverJobInfo>,
    }

    #[derive(Debug, Copy, Clone)]
    pub enum GpuProverInstanceStatus {
        // The instance is available for processing.
        Available,
        // The instance is running at full capacity.
        Full,
        // The instance is reserved by an synthesizer.
        Reserved,
        // The instance is not alive anymore.
        Dead,
    }
}

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
            // EIP 4844 are special cased.
            // There exist only 2 blobs that are calculated at basic layer and injected straight into scheduler proof (as of 1.4.2).
            // As part of 1.5.0, these will be treated as regular circuits, having basic, leaf, node and finally being attached as regular node proofs to the scheduler.
            let is_node_final_proof = *circuit_id == types::EIP_4844_CIRCUIT_ID;
            self.insert_prover_job(
                l1_batch_number,
                *circuit_id,
                depth,
                sequence_number,
                aggregation_round,
                circuit_blob_url,
                is_node_final_proof,
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
            r#"
            UPDATE prover_jobs_fri
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
                        prover_jobs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = ANY ($1)
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
            r#"
            UPDATE prover_jobs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                processing_started_at = NOW(),
                updated_at = NOW(),
                picked_by = $4
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
                                AND pj.protocol_version = ANY ($3)
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
                r#"
                UPDATE prover_jobs_fri
                SET
                    status = 'failed',
                    error = $1,
                    updated_at = NOW()
                WHERE
                    id = $2
                "#,
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
            r#"
            SELECT
                attempts
            FROM
                prover_jobs_fri
            WHERE
                id = $1
            "#,
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
            id as i64,
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
                            updated_at
                        )
                    VALUES
                        ($1, $2, $3, $4, $5, $6, $7, $8, 'queued', NOW(), NOW())
                    ON CONFLICT (l1_batch_number, aggregation_round, circuit_id, depth, sequence_number) DO
                    UPDATE
                    SET
                        updated_at = NOW()
                    "#,
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
                SELECT
                    COUNT(*) AS "count!",
                    circuit_id AS "circuit_id!",
                    aggregation_round AS "aggregation_round!",
                    status AS "status!"
                FROM
                    prover_jobs_fri
                WHERE
                    status <> 'skipped'
                    AND status <> 'successful'
                GROUP BY
                    circuit_id,
                    aggregation_round,
                    status
                "#
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.circuit_id,
                    row.aggregation_round,
                    row.status,
                    row.count as usize,
                )
            })
            .fold(
                HashMap::new(),
                |mut acc, (circuit_id, aggregation_round, status, value)| {
                    let stats = acc
                        .entry((circuit_id as u8, aggregation_round as u8))
                        .or_insert(JobCountStatistics {
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
                },
            )
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
            "#,
            status,
            id as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn save_successful_sent_proof(&mut self, l1_batch_number: L1BatchNumber) {
        sqlx::query!(
            r#"
            UPDATE prover_jobs_fri
            SET
                status = 'sent_to_server',
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
            "#,
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
            AggregationRound::Scheduler as i16,
        )
        .fetch_optional(self.storage.conn())
        .await
        .ok()?
        .map(|row| row.id as u32)
    }
}
