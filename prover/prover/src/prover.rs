use std::{env, time::Duration};

use prover_service::JobResult::{Failure, ProofGenerated};
use prover_service::{JobReporter, JobResult};
use tokio::runtime::Handle;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncProof;
use zkevm_test_harness::pairing::bn256::Bn256;

use zksync_config::ProverConfig;
use zksync_dal::StorageProcessor;
use zksync_dal::{connection::DbVariant, ConnectionPool};
use zksync_object_store::{Bucket, ObjectStore, ObjectStoreFactory};
use zksync_types::proofs::ProverJobMetadata;

#[derive(Debug)]
pub struct ProverReporter {
    rt_handle: Handle,
    pool: ConnectionPool,
    config: ProverConfig,
    processed_by: String,
    object_store: Box<dyn ObjectStore>,
}

fn assembly_debug_blob_url(job_id: usize, circuit_id: u8) -> String {
    format!("assembly_debugging_{}_{}.bin", job_id, circuit_id)
}

impl ProverReporter {
    pub(crate) fn new(
        config: ProverConfig,
        store_factory: &ObjectStoreFactory,
        rt_handle: Handle,
    ) -> Self {
        let pool = rt_handle.block_on(ConnectionPool::singleton(DbVariant::Prover).build());
        Self {
            pool,
            config,
            processed_by: env::var("POD_NAME").unwrap_or("Unknown".to_string()),
            object_store: rt_handle.block_on(store_factory.create_store()),
            rt_handle,
        }
    }

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
            "Successfully generated proof with id {:?} and type: {} for index: {}. Size: {:?}KB took: {:?}",
            job_id,
            circuit_type,
            index,
            serialized.len() >> 10,
            duration,
        );
        metrics::histogram!(
            "server.prover.proof_generation_time",
            duration,
            "circuit_type" => circuit_type,
        );
        let job_id = job_id as u32;
        self.rt_handle.block_on(async {
            let mut connection = self.pool.access_storage().await;
            let mut transaction = connection.start_transaction().await;

            // BEWARE, HERE BE DRAGONS.
            // `send_report` method is called in an operating system thread,
            // which is in charge of saving proof output (ok, errored, etc.).
            // The code that calls it is in a thread that does not check it's status.
            // If the thread panics, proofs will be generated, but their status won't be saved.
            // So a prover will work like this:
            // Pick task, execute task, prepare task to be saved, be restarted as nothing happens.
            // The error prevents the "fake" work by killing the prover, which causes it to restart.
            // A proper fix would be to have the thread signal it was dead or be watched from outside.
            // Given we want to deprecate old prover, this is the quick and dirty hack I'm not proud of.
            let result = transaction
                .prover_dal()
                .save_proof(job_id, duration, serialized, &self.processed_by)
                .await;
            if let Err(e) = result {
                vlog::warn!("panicked inside heavy-ops thread: {e:?}; exiting...");
                std::process::exit(-1);
            }
            self.get_prover_job_metadata_by_id_and_exit_if_error(&mut transaction, job_id)
                .await;
            transaction.commit().await;
        });
    }

    fn get_circuit_type(&self, job_id: usize) -> String {
        let prover_job_metadata = self.rt_handle.block_on(async {
            let mut connection = self.pool.access_storage().await;
            self.get_prover_job_metadata_by_id_and_exit_if_error(&mut connection, job_id as u32)
                .await
        });
        prover_job_metadata.circuit_type
    }

    async fn get_prover_job_metadata_by_id_and_exit_if_error(
        &self,
        connection: &mut StorageProcessor<'_>,
        job_id: u32,
    ) -> ProverJobMetadata {
        // BEWARE, HERE BE DRAGONS.
        // `send_report` method is called in an operating system thread,
        // which is in charge of saving proof output (ok, errored, etc.).
        // The code that calls it is in a thread that does not check it's status.
        // If the thread panics, proofs will be generated, but their status won't be saved.
        // So a prover will work like this:
        // Pick task, execute task, prepare task to be saved, be restarted as nothing happens.
        // The error prevents the "fake" work by killing the prover, which causes it to restart.
        // A proper fix would be to have the thread signal it was dead or be watched from outside.
        // Given we want to deprecate old prover, this is the quick and dirty hack I'm not proud of.
        let result = connection.prover_dal().get_prover_job_by_id(job_id).await;
        let prover_job_metadata = match result {
            Ok(option) => option,
            Err(e) => {
                vlog::warn!("panicked inside heavy-ops thread: {e:?}; exiting...");
                std::process::exit(-1);
            }
        };
        match prover_job_metadata {
            Some(val) => val,
            None => {
                vlog::error!("No job with id: {} exist; exiting...", job_id);
                std::process::exit(-1);
            }
        }
    }
}

impl JobReporter for ProverReporter {
    fn send_report(&mut self, report: JobResult) {
        match report {
            Failure(job_id, error) => {
                vlog::error!(
                    "Failed to generate proof for id {:?}. error reason; {}",
                    job_id,
                    error
                );
                self.rt_handle.block_on(async {
                    let result = self
                        .pool
                        .access_storage()
                        .await
                        .prover_dal()
                        .save_proof_error(job_id as u32, error, self.config.max_attempts)
                        .await;
                    // BEWARE, HERE BE DRAGONS.
                    // `send_report` method is called in an operating system thread,
                    // which is in charge of saving proof output (ok, errored, etc.).
                    // The code that calls it is in a thread that does not check it's status.
                    // If the thread panics, proofs will be generated, but their status won't be saved.
                    // So a prover will work like this:
                    // Pick task, execute task, prepare task to be saved, be restarted as nothing happens.
                    // The error prevents the "fake" work by killing the prover, which causes it to restart.
                    // A proper fix would be to have the thread signal it was dead or be watched from outside.
                    // Given we want to deprecate old prover, this is the quick and dirty hack I'm not proud of.
                    if let Err(e) = result {
                        vlog::warn!("panicked inside heavy-ops thread: {e:?}; exiting...");
                        std::process::exit(-1);
                    }
                });
            }

            ProofGenerated(job_id, duration, proof, index) => {
                self.handle_successful_proof_generation(job_id, proof, duration, index);
            }

            JobResult::Synthesized(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::trace!(
                    "Successfully synthesized circuit with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                metrics::histogram!(
                    "server.prover.circuit_synthesis_time",
                    duration,
                    "circuit_type" => circuit_type,
                );
            }

            JobResult::AssemblyFinalized(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::trace!(
                    "Successfully finalized assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                metrics::histogram!(
                    "server.prover.assembly_finalize_time",
                    duration,
                    "circuit_type" => circuit_type,
                );
            }

            JobResult::SetupLoaded(job_id, duration, cache_miss) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::trace!(
                    "Successfully setup loaded with id {:?} and type: {}. \
                     took: {:?} and had cache_miss: {}",
                    job_id,
                    circuit_type,
                    duration,
                    cache_miss
                );
                metrics::histogram!(
                    "server.prover.setup_load_time",
                    duration,
                    "circuit_type" => circuit_type.clone()
                );
                metrics::counter!(
                    "server.prover.setup_loading_cache_miss",
                    1,
                    "circuit_type" => circuit_type
                );
            }

            JobResult::AssemblyEncoded(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::trace!(
                    "Successfully encoded assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                metrics::histogram!(
                    "server.prover.assembly_encoding_time",
                    duration,
                    "circuit_type" => circuit_type,
                );
            }

            JobResult::AssemblyDecoded(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::trace!(
                    "Successfully decoded assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                metrics::histogram!(
                    "server.prover.assembly_decoding_time",
                    duration,
                    "circuit_type" => circuit_type,
                );
            }

            JobResult::FailureWithDebugging(job_id, circuit_id, assembly, error) => {
                vlog::trace!(
                    "Failed assembly decoding for job-id {} and circuit-type: {}. error: {}",
                    job_id,
                    circuit_id,
                    error,
                );
                let blob_url = assembly_debug_blob_url(job_id, circuit_id);
                let put_task = self
                    .object_store
                    .put_raw(Bucket::ProverJobs, &blob_url, assembly);
                self.rt_handle
                    .block_on(put_task)
                    .expect("Failed saving debug assembly to GCS");
            }

            JobResult::AssemblyTransferred(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                vlog::trace!(
                    "Successfully transferred assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                metrics::histogram!(
                    "server.prover.assembly_transferring_time",
                    duration,
                    "circuit_type" => circuit_type,
                );
            }

            JobResult::ProverWaitedIdle(prover_id, duration) => {
                vlog::trace!(
                    "Prover wait idle time: {:?} for prover-id: {:?}",
                    duration,
                    prover_id
                );
                metrics::histogram!("server.prover.prover_wait_idle_time", duration,);
            }

            JobResult::SetupLoaderWaitedIdle(duration) => {
                vlog::trace!("Setup load wait idle time: {:?}", duration);
                metrics::histogram!("server.prover.setup_load_wait_wait_idle_time", duration,);
            }

            JobResult::SchedulerWaitedIdle(duration) => {
                vlog::trace!("Scheduler wait idle time: {:?}", duration);
                metrics::histogram!("server.prover.scheduler_wait_idle_time", duration,);
            }
        }
    }
}
