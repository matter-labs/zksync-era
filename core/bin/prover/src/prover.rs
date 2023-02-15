use std::time::Duration;

use prover_service::JobResult::{Failure, ProofGenerated};
use prover_service::{JobReporter, JobResult};
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::{ZkSyncProof};
use zkevm_test_harness::pairing::bn256::Bn256;


use zksync_config::ProverConfig;
use zksync_dal::ConnectionPool;

use zksync_object_store::object_store::{create_object_store_from_env, PROVER_JOBS_BUCKET_PATH};


#[derive(Debug)]
pub struct ProverReporter {
    pub(crate) pool: ConnectionPool,
    pub(crate) config: ProverConfig,
    pub(crate) processed_by: String,
}

pub fn assembly_debug_blob_url(job_id: usize, circuit_id: u8) -> String {
    format!("assembly_debugging_{}_{}.bin", job_id, circuit_id)
}

impl ProverReporter {
    fn handle_successful_proof_generation(
        &self,
        job_id: usize,
        proof: ZkSyncProof<Bn256>,
        duration: Duration,
        index: usize,
    ) {
        let circuit_type = self.get_circuit_type(job_id);
        let serialized = bincode::serialize(&proof).expect("Failed to serialize proof");
        vlog::info!(
            "Successfully generated proof with id {:?} and type: {} for index: {}. Size: {:?}KB took: {}",
            job_id,
            circuit_type.clone(),
            index,
            serialized.len() >> 10,
            duration.as_secs() as f64,
        );
        metrics::histogram!(
            "server.prover.proof_generation_time",
            duration.as_secs() as f64,
            "circuit_type" => circuit_type,
        );
        let job_id = job_id as u32;
        let mut connection = self.pool.access_storage_blocking();
        let mut transaction = connection.start_transaction_blocking();

        // Lock `prover_jobs` table.
        // It is needed to have only one transaction at the moment
        // that calls `successful_proofs_count` method to avoid race condition.
        transaction.prover_dal().lock_prover_jobs_table_exclusive();
        transaction
            .prover_dal()
            .save_proof(job_id, duration, serialized, &self.processed_by);
        let prover_job_metadata = transaction
            .prover_dal()
            .get_prover_job_by_id(job_id)
            .unwrap_or_else(|| panic!("No job with id: {} exist", job_id));

        if let Some(next_round) = prover_job_metadata.aggregation_round.next() {
            // for Basic, Leaf and Node rounds we need to mark the next job as `queued`
            // if all the dependent proofs are computed

            let successful_proofs_count = transaction.prover_dal().successful_proofs_count(
                prover_job_metadata.block_number,
                prover_job_metadata.aggregation_round,
            );

            let required_proofs_count = transaction
                .witness_generator_dal()
                .required_proofs_count(prover_job_metadata.block_number, next_round);

            vlog::info!(
                "Generated {}/{} {:?} circuits of block {:?}",
                successful_proofs_count,
                required_proofs_count,
                prover_job_metadata.aggregation_round,
                prover_job_metadata.block_number.0
            );

            if successful_proofs_count == required_proofs_count {
                transaction
                    .witness_generator_dal()
                    .mark_witness_job_as_queued(prover_job_metadata.block_number, next_round);
            }
        } else {
            let block = transaction
                .blocks_dal()
                .get_block_header(prover_job_metadata.block_number)
                .unwrap();
            metrics::counter!(
                "server.processed_txs",
                block.tx_count() as u64,
                "stage" => "prove_generated"
            );
        }
        transaction.commit_blocking();
        metrics::gauge!(
            "server.block_number",
            prover_job_metadata.block_number.0 as f64,
            "stage" =>  format!("prove_{:?}",prover_job_metadata.aggregation_round),
        );
    }

    fn get_circuit_type(&self, job_id: usize) -> String {
        let prover_job_metadata = self
            .pool
            .access_storage_blocking()
            .prover_dal()
            .get_prover_job_by_id(job_id as u32)
            .unwrap_or_else(|| panic!("No job with id: {} exist", job_id));
        prover_job_metadata.circuit_type
    }
}

impl JobReporter for ProverReporter {
    fn send_report(&mut self, report: JobResult) {
        match report {
            Failure(job_id, error) => {
                vlog::info!(
                    "Failed to generate proof for id {:?}. error reason; {}",
                    job_id,
                    error
                );
                self.pool
                    .access_storage_blocking()
                    .prover_dal()
                    .save_proof_error(job_id as u32, error, self.config.max_attempts);
            }
            ProofGenerated(job_id, duration, proof, index) => {
                self.handle_successful_proof_generation(job_id, proof, duration, index);
            }

            JobResult::Synthesized(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::info!(
                    "Successfully synthesized circuit with id {:?} and type: {}. took: {}",
                    job_id,
                    circuit_type.clone(),
                    duration.as_secs() as f64,
                );
                metrics::histogram!(
                    "server.prover.circuit_synthesis_time",
                    duration.as_secs() as f64,
                    "circuit_type" => circuit_type,
                );
            }
            JobResult::AssemblyFinalized(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::info!(
                    "Successfully finalized assembly with id {:?} and type: {}. took: {}",
                    job_id,
                    circuit_type.clone(),
                    duration.as_secs() as f64,
                );
                metrics::histogram!(
                    "server.prover.assembly_finalize_time",
                    duration.as_secs() as f64,
                    "circuit_type" => circuit_type,
                );
            }

            JobResult::SetupLoaded(job_id, duration, cache_miss) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::info!(
                    "Successfully setup loaded with id {:?} and type: {}. took: {:?} and had cache_miss: {}",
                    job_id,
                    circuit_type.clone(),
                    duration.as_secs() as f64,
                    cache_miss
                );
                metrics::histogram!("server.prover.setup_load_time", duration.as_secs() as f64,
                    "circuit_type" => circuit_type.clone(),);
                metrics::counter!(
                    "server.prover.setup_loading_cache_miss",
                    1,
                    "circuit_type" => circuit_type
                );
            }
            JobResult::AssemblyEncoded(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::info!(
                    "Successfully encoded assembly with id {:?} and type: {}. took: {}",
                    job_id,
                    circuit_type.clone(),
                    duration.as_secs() as f64,
                );
                metrics::histogram!(
                    "server.prover.assembly_encoding_time",
                    duration.as_secs() as f64,
                    "circuit_type" => circuit_type,
                );
            }
            JobResult::AssemblyDecoded(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::info!(
                    "Successfully decoded assembly with id {:?} and type: {}. took: {}",
                    job_id,
                    circuit_type.clone(),
                    duration.as_secs() as f64,
                );
                metrics::histogram!(
                    "server.prover.assembly_decoding_time",
                    duration.as_secs() as f64,
                    "circuit_type" => circuit_type,
                );
            }
            JobResult::FailureWithDebugging(job_id, circuit_id, assembly, error) => {
                let mut object_store = create_object_store_from_env();
                vlog::info!(
                    "Failed assembly decoding for job-id {} and circuit-type: {}. error: {}",
                    job_id,
                    circuit_id,
                    error,
                );
                let blob_url = assembly_debug_blob_url(job_id, circuit_id);
                object_store
                    .put(PROVER_JOBS_BUCKET_PATH, blob_url, assembly)
                    .expect("Failed saving debug assembly to GCS");
            }
            JobResult::AssemblyTransferred(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::info!(
                    "Successfully transferred assembly with id {:?} and type: {}. took: {}",
                    job_id,
                    circuit_type.clone(),
                    duration.as_secs() as f64,
                );
                metrics::histogram!(
                    "server.prover.assembly_transferring_time",
                    duration.as_secs() as f64,
                    "circuit_type" => circuit_type,
                );
            }
            JobResult::ProverWaitedIdle(prover_id, duration) => {
                vlog::info!(
                    "Prover wait idle time: {} for prover-id: {:?}",
                    duration.as_secs() as f64,
                    prover_id
                );
                metrics::histogram!(
                    "server.prover.prover_wait_idle_time",
                    duration.as_secs() as f64,
                );
            }
            JobResult::SetupLoaderWaitedIdle(duration) => {
                vlog::info!("Setup load wait idle time: {}", duration.as_secs() as f64,);
                metrics::histogram!(
                    "server.prover.setup_load_wait_wait_idle_time",
                    duration.as_secs() as f64,
                );
            }
            JobResult::SchedulerWaitedIdle(duration) => {
                vlog::info!("Scheduler wait idle time: {}", duration.as_secs() as f64,);
                metrics::histogram!(
                    "server.prover.scheduler_wait_idle_time",
                    duration.as_secs() as f64,
                );
            }
        }
    }
}
