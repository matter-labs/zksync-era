use anyhow::Context as _;
use std::{env, time::Duration};

use prover_service::{
    JobReporter,
    JobResult::{self, Failure, ProofGenerated},
};
use tokio::runtime::Handle;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncProof;
use zkevm_test_harness::pairing::bn256::Bn256;

use crate::metrics::PROVER_METRICS;
use zksync_config::{PostgresConfig, ProverConfig};
use zksync_dal::ConnectionPool;
use zksync_dal::StorageProcessor;
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
        postgres_config: PostgresConfig,
        config: ProverConfig,
        store_factory: &ObjectStoreFactory,
        rt_handle: Handle,
    ) -> anyhow::Result<Self> {
        let pool = rt_handle
            .block_on(ConnectionPool::singleton(postgres_config.prover_url()?).build())
            .context("failed to build a connection pool")?;
        Ok(Self {
            pool,
            config,
            processed_by: env::var("POD_NAME").unwrap_or("Unknown".to_string()),
            object_store: rt_handle.block_on(store_factory.create_store()),
            rt_handle,
        })
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
        tracing::info!(
            "Successfully generated proof with id {:?} and type: {} for index: {}. Size: {:?}KB took: {:?}",
            job_id,
            circuit_type,
            index,
            serialized.len() >> 10,
            duration,
        );

        let label: &'static str = Box::leak(circuit_type.into_boxed_str());
        PROVER_METRICS.proof_generation_time[&label].observe(duration);

        let job_id = job_id as u32;
        self.rt_handle.block_on(async {
            let mut connection = self.pool.access_storage().await.unwrap();
            let mut transaction = connection.start_transaction().await.unwrap();

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
                tracing::warn!("panicked inside heavy-ops thread: {e:?}; exiting...");
                std::process::exit(-1);
            }
            self.get_prover_job_metadata_by_id_and_exit_if_error(&mut transaction, job_id)
                .await;
            transaction.commit().await.unwrap();
        });
    }

    fn get_circuit_type(&self, job_id: usize) -> String {
        let prover_job_metadata = self.rt_handle.block_on(async {
            let mut connection = self.pool.access_storage().await.unwrap();
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
                tracing::warn!("panicked inside heavy-ops thread: {e:?}; exiting...");
                std::process::exit(-1);
            }
        };
        match prover_job_metadata {
            Some(val) => val,
            None => {
                tracing::error!("No job with id: {} exist; exiting...", job_id);
                std::process::exit(-1);
            }
        }
    }
}

impl JobReporter for ProverReporter {
    fn send_report(&mut self, report: JobResult) {
        match report {
            Failure(job_id, error) => {
                tracing::error!(
                    "Failed to generate proof for id {:?}. error reason; {}",
                    job_id,
                    error
                );
                self.rt_handle.block_on(async {
                    let result = self
                        .pool
                        .access_storage()
                        .await
                        .unwrap()
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
                        tracing::warn!("panicked inside heavy-ops thread: {e:?}; exiting...");
                        std::process::exit(-1);
                    }
                });
            }

            ProofGenerated(job_id, duration, proof, index) => {
                self.handle_successful_proof_generation(job_id, proof, duration, index);
            }

            JobResult::Synthesized(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                tracing::trace!(
                    "Successfully synthesized circuit with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                let label: &'static str = Box::leak(circuit_type.into_boxed_str());
                PROVER_METRICS.circuit_synthesis_time[&label].observe(duration);
            }

            JobResult::AssemblyFinalized(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                tracing::trace!(
                    "Successfully finalized assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                let label: &'static str = Box::leak(circuit_type.into_boxed_str());
                PROVER_METRICS.assembly_finalize_time[&label].observe(duration);
            }

            JobResult::SetupLoaded(job_id, duration, cache_miss) => {
                let circuit_type = self.get_circuit_type(job_id);
                tracing::trace!(
                    "Successfully setup loaded with id {:?} and type: {}. \
                     took: {:?} and had cache_miss: {}",
                    job_id,
                    circuit_type,
                    duration,
                    cache_miss
                );
                let label: &'static str = Box::leak(circuit_type.into_boxed_str());
                PROVER_METRICS.setup_load_time[&label].observe(duration);
                PROVER_METRICS.setup_loading_cache_miss[&label].inc();
            }

            JobResult::AssemblyEncoded(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                tracing::trace!(
                    "Successfully encoded assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                let label: &'static str = Box::leak(circuit_type.into_boxed_str());
                PROVER_METRICS.assembly_encoding_time[&label].observe(duration);
            }

            JobResult::AssemblyDecoded(job_id, duration) => {
                let circuit_type = self.get_circuit_type(job_id);
                tracing::trace!(
                    "Successfully decoded assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                let label: &'static str = Box::leak(circuit_type.into_boxed_str());
                PROVER_METRICS.assembly_decoding_time[&label].observe(duration);
            }

            JobResult::FailureWithDebugging(job_id, circuit_id, assembly, error) => {
                tracing::trace!(
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
                tracing::trace!(
                    "Successfully transferred assembly with id {:?} and type: {}. took: {:?}",
                    job_id,
                    circuit_type,
                    duration,
                );
                let label: &'static str = Box::leak(circuit_type.into_boxed_str());
                PROVER_METRICS.assembly_transferring_time[&label].observe(duration);
            }

            JobResult::ProverWaitedIdle(prover_id, duration) => {
                tracing::trace!(
                    "Prover wait idle time: {:?} for prover-id: {:?}",
                    duration,
                    prover_id
                );
                PROVER_METRICS.prover_wait_idle_time.observe(duration);
            }

            JobResult::SetupLoaderWaitedIdle(duration) => {
                tracing::trace!("Setup load wait idle time: {:?}", duration);
                PROVER_METRICS.setup_load_wait_idle_time.observe(duration);
            }

            JobResult::SchedulerWaitedIdle(duration) => {
                tracing::trace!("Scheduler wait idle time: {:?}", duration);
                PROVER_METRICS.scheduler_wait_idle_time.observe(duration);
            }
        }
    }
}
