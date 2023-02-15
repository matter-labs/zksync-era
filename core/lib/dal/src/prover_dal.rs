use std::collections::HashMap;
use std::convert::TryFrom;
use std::ops::Range;
use std::time::{Duration, Instant};

use zksync_object_store::gcs_utils::prover_circuit_input_blob_url;
use zksync_types::aggregated_operations::BlockProofForL1;
use zksync_types::proofs::{
    AggregationRound, JobCountStatistics, JobExtendedStatistics, ProverJobInfo, ProverJobMetadata,
};
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncProof;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::L1BatchNumber;

use crate::models::storage_prover_job_info::StorageProverJobInfo;
use crate::time_utils::{duration_to_naive_time, pg_interval_from_duration};
use crate::StorageProcessor;

#[derive(Debug)]
pub struct ProverDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ProverDal<'_, '_> {
    pub fn get_next_prover_job(
        &mut self,
        _processing_timeout: Duration,
        max_attempts: u32,
    ) -> Option<ProverJobMetadata> {
        async_std::task::block_on(async {
            let processing_timeout = pg_interval_from_duration(_processing_timeout);
            let result: Option<ProverJobMetadata> = sqlx::query!(
                "
                UPDATE prover_jobs
                SET status = 'in_progress', attempts = attempts + 1,
                    updated_at = now(), processing_started_at = now()
                WHERE id = (
                    SELECT id
                    FROM prover_jobs
                    WHERE status = 'queued' 
                    OR (status = 'in_progress' AND  processing_started_at < now() - $1::interval)
                    OR (status = 'in_gpu_proof' AND  processing_started_at < now() - $1::interval)
                    OR (status = 'failed' AND attempts < $2)
                    ORDER BY aggregation_round DESC, l1_batch_number ASC, id ASC
                    LIMIT 1
                    FOR UPDATE
                    SKIP LOCKED
                )
                RETURNING prover_jobs.*
                ",
                &processing_timeout,
                max_attempts as i32
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|row| ProverJobMetadata {
                id: row.id as u32,
                block_number: L1BatchNumber(row.l1_batch_number as u32),
                circuit_type: row.circuit_type.clone(),
                aggregation_round: AggregationRound::try_from(row.aggregation_round).unwrap(),
                sequence_number: row.sequence_number as usize,
            });
            result
        })
    }

    pub fn get_next_prover_job_by_circuit_types(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
        circuit_types: Vec<String>,
    ) -> Option<ProverJobMetadata> {
        async_std::task::block_on(async {
            let processing_timeout = pg_interval_from_duration(processing_timeout);
            let result: Option<ProverJobMetadata> = sqlx::query!(
                "
                UPDATE prover_jobs
                SET status = 'in_progress', attempts = attempts + 1,
                    updated_at = now(), processing_started_at = now()
                WHERE id = (
                    SELECT id
                    FROM prover_jobs
                    WHERE circuit_type = ANY($3)
                    AND
                    (   status = 'queued'
                        OR (status = 'in_progress' AND  processing_started_at < now() - $1::interval)
                        OR (status = 'failed' AND attempts < $2)
                    )
                    ORDER BY aggregation_round DESC, l1_batch_number ASC, id ASC
                    LIMIT 1
                    FOR UPDATE
                    SKIP LOCKED
                )
                RETURNING prover_jobs.*
                ",
                &processing_timeout,
                max_attempts as i32,
                &circuit_types[..],
            )
                .fetch_optional(self.storage.conn())
                .await
                .unwrap()
                .map(|row| ProverJobMetadata {
                    id: row.id as u32,
                    block_number: L1BatchNumber(row.l1_batch_number as u32),
                    circuit_type: row.circuit_type,
                    aggregation_round: AggregationRound::try_from(row.aggregation_round).unwrap(),
                    sequence_number: row.sequence_number as usize,
                });

            result
        })
    }

    // If making changes to this method, consider moving the serialization logic to the DAL layer.
    pub fn insert_prover_jobs(
        &mut self,
        l1_batch_number: L1BatchNumber,
        circuits: Vec<String>,
        aggregation_round: AggregationRound,
    ) {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            for (sequence_number, circuit) in circuits.into_iter().enumerate() {
                let circuit_input_blob_url = prover_circuit_input_blob_url(
                    l1_batch_number,
                    sequence_number,
                    circuit.clone(),
                    aggregation_round,
                );
                sqlx::query!(
                    "
                    INSERT INTO prover_jobs (l1_batch_number, circuit_type, sequence_number, prover_input, aggregation_round, circuit_input_blob_url, status, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, 'queued', now(), now())
                    ON CONFLICT(l1_batch_number, aggregation_round, sequence_number) DO NOTHING
                    ",
                    l1_batch_number.0 as i64,
                    circuit,
                    sequence_number as i64,
                    vec![],
                    aggregation_round as i64,
                    circuit_input_blob_url
                )
                    .execute(self.storage.conn())
                    .await
                    .unwrap();
                metrics::histogram!("dal.request", started_at.elapsed(), "method" => "save_witness");
            }
        })
    }

    pub fn save_proof(
        &mut self,
        id: u32,
        time_taken: Duration,
        proof: Vec<u8>,
        proccesed_by: &str,
    ) {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            sqlx::query!(
                "
                UPDATE prover_jobs
                SET status = 'successful', updated_at = now(), time_taken = $1, result = $2, proccesed_by = $3
                WHERE id = $4
                ",
                duration_to_naive_time(time_taken),
                proof,
                proccesed_by,
                id as i64,
            )
                .execute(self.storage.conn())
                .await
                .unwrap();

            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "save_proof");
        })
    }

    pub fn lock_prover_jobs_table_exclusive(&mut self) {
        async_std::task::block_on(async {
            sqlx::query!("LOCK TABLE prover_jobs IN EXCLUSIVE MODE")
                .execute(self.storage.conn())
                .await
                .unwrap();
        })
    }

    pub fn save_proof_error(&mut self, id: u32, error: String, max_attempts: u32) {
        async_std::task::block_on(async {
            let mut transaction = self.storage.start_transaction().await;

            let row = sqlx::query!(
                "
                UPDATE prover_jobs
                SET status = 'failed', error = $1, updated_at = now()
                WHERE id = $2
                RETURNING l1_batch_number, attempts
                ",
                error,
                id as i64,
            )
            .fetch_one(transaction.conn())
            .await
            .unwrap();

            if row.attempts as u32 >= max_attempts {
                transaction
                    .blocks_dal()
                    .set_skip_proof_for_l1_batch(L1BatchNumber(row.l1_batch_number as u32));
            }

            transaction.commit().await;
        })
    }

    // For each block in the provided range it returns a tuple:
    // (aggregation_coords; scheduler_proof)
    pub fn get_final_proofs_for_blocks(
        &mut self,
        from_block: L1BatchNumber,
        to_block: L1BatchNumber,
    ) -> Vec<BlockProofForL1> {
        async_std::task::block_on(async {
            sqlx::query!(
                "SELECT prover_jobs.result as proof, scheduler_witness_jobs.aggregation_result_coords
                FROM prover_jobs
                INNER JOIN scheduler_witness_jobs
                ON prover_jobs.l1_batch_number = scheduler_witness_jobs.l1_batch_number
                WHERE prover_jobs.l1_batch_number >= $1 AND prover_jobs.l1_batch_number <= $2
                AND prover_jobs.aggregation_round = 3
                AND prover_jobs.status = 'successful'
                AND scheduler_witness_jobs.status = 'successful'
                ",
                from_block.0 as i32,
                to_block.0 as i32
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .map(|row| {
                    let deserialized_proof = bincode::deserialize::<ZkSyncProof<Bn256>>(
                        &row.proof
                            .expect("prove_job with `successful` status has no result"),
                    ).expect("cannot deserialize proof");
                    let deserialized_aggregation_result_coords = bincode::deserialize::<[[u8; 32]; 4]>(
                        &row.aggregation_result_coords
                            .expect("scheduler_witness_job with `successful` status has no aggregation_result_coords"),
                    ).expect("cannot deserialize proof");
                    BlockProofForL1 {
                        aggregation_result_coords: deserialized_aggregation_result_coords,
                        scheduler_proof: ZkSyncProof::into_proof(deserialized_proof),
                    }
                })
                .collect()
        })
    }

    pub fn get_prover_jobs_stats(&mut self) -> JobCountStatistics {
        async_std::task::block_on(async {
            let mut results: HashMap<String, usize> = sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!", status as "status!"
                FROM prover_jobs
                GROUP BY status
                "#
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.status, row.count as usize))
            .collect::<HashMap<String, usize>>();
            JobCountStatistics {
                queued: results.remove("queued").unwrap_or(0usize),
                in_progress: results.remove("in_progress").unwrap_or(0usize),
                failed: results.remove("failed").unwrap_or(0usize),
                successful: results.remove("successful").unwrap_or(0usize),
            }
        })
    }

    pub fn successful_proofs_count(
        &mut self,
        block_number: L1BatchNumber,
        aggregation_round: AggregationRound,
    ) -> usize {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!"
                FROM prover_jobs
                WHERE status = 'successful' AND l1_batch_number = $1 AND aggregation_round = $2
                "#,
                block_number.0 as i64,
                aggregation_round as i64
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count as usize
        })
    }

    pub fn min_unproved_l1_batch_number(&mut self, max_attempts: u32) -> Option<L1BatchNumber> {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                SELECT MIN(l1_batch_number) as "l1_batch_number?"
                FROM prover_jobs
                WHERE status = 'queued' OR status = 'in_progress'
                OR status = 'in_gpu_proof'
                    OR (status = 'failed' AND attempts < $1)
                "#,
                max_attempts as i32
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .l1_batch_number
            .map(|n| L1BatchNumber(n as u32))
        })
    }

    pub fn get_extended_stats(&mut self) -> anyhow::Result<JobExtendedStatistics> {
        async_std::task::block_on(async {
            let limits = sqlx::query!(
                r#"
                SELECT
                    (SELECT l1_batch_number
                    FROM prover_jobs
                    WHERE status NOT IN ('successful', 'skipped')
                    ORDER BY l1_batch_number
                    LIMIT 1) as "successful_limit!",
                    
                    (SELECT l1_batch_number
                    FROM prover_jobs
                    WHERE status <> 'queued'
                    ORDER BY l1_batch_number DESC
                    LIMIT 1) as "queued_limit!",

                    (SELECT MAX(l1_batch_number) as "max!" FROM prover_jobs) as "max_block!"
                "#
            )
            .fetch_one(self.storage.conn())
            .await?;

            let active_area = self.get_jobs(GetProverJobsParams::blocks(
                L1BatchNumber(limits.successful_limit as u32)
                    ..L1BatchNumber(limits.queued_limit as u32),
            ))?;

            Ok(JobExtendedStatistics {
                successful_padding: L1BatchNumber(limits.successful_limit as u32 - 1),
                queued_padding: L1BatchNumber(limits.queued_limit as u32 + 1),
                queued_padding_len: (limits.max_block - limits.queued_limit) as u32,
                active_area,
            })
        })
    }

    pub fn get_jobs(
        &mut self,
        opts: GetProverJobsParams,
    ) -> Result<Vec<ProverJobInfo>, sqlx::Error> {
        let statuses = opts
            .statuses
            .map(|ss| {
                {
                    // Until statuses are enums
                    let whitelist = vec!["queued", "in_progress", "successful", "failed"];
                    if !ss.iter().all(|x| whitelist.contains(&x.as_str())) {
                        panic!("Forbidden value in statuses list.")
                    }
                }

                format!(
                    "AND status IN ({})",
                    ss.iter()
                        .map(|x| format!("'{}'", x))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .unwrap_or_default();

        let block_range = opts
            .blocks
            .as_ref()
            .map(|range| {
                format!(
                    "AND l1_batch_number >= {}
                     AND l1_batch_number <= {}",
                    range.start.0, range.end.0
                )
            })
            .unwrap_or_default();

        let round = opts
            .round
            .map(|round| format!("AND aggregation_round = {}", round as u32))
            .unwrap_or_default();

        let order = match opts.desc {
            true => "DESC",
            false => "ASC",
        };

        let limit = opts
            .limit
            .map(|limit| format!("LIMIT {}", limit))
            .unwrap_or_default();

        let sql = format!(
            r#"
            SELECT
                id,
                circuit_type,
                l1_batch_number,
                status,
                aggregation_round,
                sequence_number,
                length(prover_input) as input_length,
                attempts,
                created_at,
                updated_at,
                processing_started_at,
                time_taken,
                error
            FROM prover_jobs
            WHERE 1 = 1 -- Where clause can't be empty
            {statuses}
            {block_range}
            {round}
            ORDER BY "id" {order}
            {limit}
            "#
        );

        let query = sqlx::query_as(&sql);

        Ok(
            async_std::task::block_on(async move { query.fetch_all(self.storage.conn()).await })?
                .into_iter()
                .map(|x: StorageProverJobInfo| x.into())
                .collect::<Vec<_>>(),
        )
    }

    pub fn get_prover_job_by_id(&mut self, job_id: u32) -> Option<ProverJobMetadata> {
        async_std::task::block_on(async {
            let result: Option<ProverJobMetadata> =
                sqlx::query!("SELECT * from prover_jobs where id=$1", job_id as i64)
                    .fetch_optional(self.storage.conn())
                    .await
                    .unwrap()
                    .map(|row| ProverJobMetadata {
                        id: row.id as u32,
                        block_number: L1BatchNumber(row.l1_batch_number as u32),
                        circuit_type: row.circuit_type.clone(),
                        aggregation_round: AggregationRound::try_from(row.aggregation_round)
                            .unwrap(),
                        sequence_number: row.sequence_number as usize,
                    });
            result
        })
    }

    pub fn get_l1_batches_with_blobs_in_db(&mut self, limit: u8) -> Vec<i64> {
        async_std::task::block_on(async {
            let job_ids = sqlx::query!(
                r#"
                    SELECT id FROM prover_jobs
                    WHERE length(prover_input) <> 0
                    LIMIT $1;
                "#,
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            job_ids.into_iter().map(|row| row.id).collect()
        })
    }

    pub fn get_circuit_input_blob_urls_to_be_cleaned(&mut self, limit: u8) -> Vec<(i64, String)> {
        async_std::task::block_on(async {
            let job_ids = sqlx::query!(
                r#"
                    SELECT id, circuit_input_blob_url FROM prover_jobs
                    WHERE status='successful' AND is_blob_cleaned=FALSE
                    AND circuit_input_blob_url is NOT NULL
                    AND updated_at < NOW() - INTERVAL '2 days'
                    LIMIT $1;
                "#,
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            job_ids
                .into_iter()
                .map(|row| (row.id, row.circuit_input_blob_url.unwrap()))
                .collect()
        })
    }

    pub fn mark_gcs_blobs_as_cleaned(&mut self, ids: Vec<i64>) {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                UPDATE prover_jobs
                SET is_blob_cleaned=TRUE
                WHERE id = ANY($1);
            "#,
                &ids[..]
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn purge_blobs_from_db(&mut self, job_ids: Vec<i64>) {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                UPDATE prover_jobs
                SET prover_input=''
                WHERE id = ANY($1);
            "#,
                &job_ids[..]
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn update_status(&mut self, id: u32, status: &str) {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                UPDATE prover_jobs
                SET status = $1, updated_at = now()
                WHERE id = $2
                "#,
                status,
                id as i64,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }
}

pub struct GetProverJobsParams {
    pub statuses: Option<Vec<String>>,
    pub blocks: Option<Range<L1BatchNumber>>,
    pub limit: Option<u32>,
    pub desc: bool,
    pub round: Option<AggregationRound>,
}

impl GetProverJobsParams {
    pub fn blocks(range: Range<L1BatchNumber>) -> GetProverJobsParams {
        GetProverJobsParams {
            blocks: Some(range),
            statuses: None,
            limit: None,
            desc: false,
            round: None,
        }
    }
}
