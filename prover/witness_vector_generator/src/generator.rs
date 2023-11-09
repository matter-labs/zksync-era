use std::time::{Duration, Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::task::JoinHandle;

use tokio::time::sleep;
use zksync_config::configs::FriWitnessVectorGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::{CircuitWrapper, ProverJob, WitnessVectorArtifacts};
use zksync_prover_fri_utils::fetch_next_circuit;
use zksync_prover_fri_utils::get_numeric_circuit_id;
use zksync_prover_fri_utils::socket_utils::send_assembly;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::basic_fri_types::CircuitIdRoundTuple;
use zksync_types::proofs::{GpuProverInstanceStatus, SocketAddress};
use zksync_types::protocol_version::L1VerifierConfig;
use zksync_vk_setup_data_server_fri::get_finalization_hints;

pub struct WitnessVectorGenerator {
    blob_store: Box<dyn ObjectStore>,
    pool: ConnectionPool,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
    zone: String,
    config: FriWitnessVectorGeneratorConfig,
    vk_commitments: L1VerifierConfig,
    max_attempts: u32,
}

impl WitnessVectorGenerator {
    pub fn new(
        blob_store: Box<dyn ObjectStore>,
        prover_connection_pool: ConnectionPool,
        circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
        zone: String,
        config: FriWitnessVectorGeneratorConfig,
        vk_commitments: L1VerifierConfig,
        max_attempts: u32,
    ) -> Self {
        Self {
            blob_store,
            pool: prover_connection_pool,
            circuit_ids_for_round_to_be_proven,
            zone,
            config,
            vk_commitments,
            max_attempts,
        }
    }

    pub fn generate_witness_vector(job: ProverJob) -> anyhow::Result<WitnessVectorArtifacts> {
        let finalization_hints = get_finalization_hints(job.setup_data_key.clone())
            .context("get_finalization_hints()")?;
        let mut cs = match job.circuit_wrapper.clone() {
            CircuitWrapper::Base(base_circuit) => {
                base_circuit.synthesis::<GoldilocksField>(&finalization_hints)
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                recursive_circuit.synthesis::<GoldilocksField>(&finalization_hints)
            }
        };
        Ok(WitnessVectorArtifacts::new(
            cs.materialize_witness_vec(),
            job,
        ))
    }
}

#[async_trait]
impl JobProcessor for WitnessVectorGenerator {
    type Job = ProverJob;
    type JobId = u32;
    type JobArtifacts = WitnessVectorArtifacts;
    const SERVICE_NAME: &'static str = "WitnessVectorGenerator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut storage = self.pool.access_storage().await.unwrap();
        let Some(job) = fetch_next_circuit(
            &mut storage,
            &*self.blob_store,
            &self.circuit_ids_for_round_to_be_proven,
            &self.vk_commitments,
        )
        .await
        else {
            return Ok(None);
        };
        Ok(Some((job.job_id, job)))
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        self.pool
            .access_storage()
            .await
            .unwrap()
            .fri_prover_jobs_dal()
            .save_proof_error(job_id, error)
            .await;
    }

    async fn process_job(
        &self,
        job: ProverJob,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        tokio::task::spawn_blocking(move || Self::generate_witness_vector(job))
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: WitnessVectorArtifacts,
    ) -> anyhow::Result<()> {
        metrics::histogram!(
            "prover_fri.witness_vector_generator.gpu_witness_vector_generation_time",
            started_at.elapsed(),
            "circuit_type" => get_numeric_circuit_id(&artifacts.prover_job.circuit_wrapper).to_string(),
        );
        tracing::info!(
            "Finished witness vector generation for job: {job_id} in zone: {:?} took: {:?}",
            self.zone,
            started_at.elapsed()
        );

        let serialized: Vec<u8> =
            bincode::serialize(&artifacts).expect("Failed to serialize witness vector artifacts");

        let now = Instant::now();
        let mut attempts = 0;

        while now.elapsed() < self.config.prover_instance_wait_timeout() {
            let prover = self
                .pool
                .access_storage()
                .await
                .unwrap()
                .fri_gpu_prover_queue_dal()
                .lock_available_prover(
                    self.config.max_prover_reservation_duration(),
                    self.config.specialized_group_id,
                    self.zone.clone(),
                )
                .await;

            if let Some(address) = prover {
                let result = send_assembly(job_id, &serialized, &address);
                handle_send_result(&result, job_id, &address, &self.pool, self.zone.clone()).await;

                if result.is_ok() {
                    return Ok(());
                }

                tracing::warn!(
                    "Could not send witness vector to {address:?}. Prover group {}, zone {}, \
                         job {job_id}, send attempt {attempts}.",
                    self.config.specialized_group_id,
                    self.zone,
                );
                attempts += 1;
            } else {
                sleep(self.config.prover_instance_poll_time()).await;
            }
        }
        tracing::trace!(
            "Not able to get any free prover instance for sending witness vector for job: {job_id}"
        );
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for WitnessVectorGenerator")?;
        prover_storage
            .fri_prover_jobs_dal()
            .get_prover_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for WitnessVectorGenerator")
    }
}

async fn handle_send_result(
    result: &Result<(Duration, u64), String>,
    job_id: u32,
    address: &SocketAddress,
    pool: &ConnectionPool,
    zone: String,
) {
    match result {
        Ok((elapsed, len)) => {
            let blob_size_in_mb = len / (1024 * 1024);

            tracing::info!(
                "Sent assembly of size: {blob_size_in_mb}MB successfully, took: {elapsed:?} \
                 for job: {job_id} to: {address:?}"
            );
            metrics::histogram!(
                "prover_fri.witness_vector_generator.blob_sending_time",
                *elapsed,
                "blob_size_in_mb" => blob_size_in_mb.to_string(),
            );

            pool.access_storage()
                .await
                .unwrap()
                .fri_prover_jobs_dal()
                .update_status(job_id, "in_gpu_proof")
                .await;
        }

        Err(err) => {
            tracing::trace!(
                "Failed sending assembly to address: {address:?}, socket not reachable \
                 reason: {err}"
            );

            // mark prover instance in gpu_prover_queue dead
            pool.access_storage()
                .await
                .unwrap()
                .fri_gpu_prover_queue_dal()
                .update_prover_instance_status(address.clone(), GpuProverInstanceStatus::Dead, zone)
                .await;

            // mark the job as failed
            pool.access_storage()
                .await
                .unwrap()
                .fri_prover_jobs_dal()
                .save_proof_error(job_id, "prover instance unreachable".to_string())
                .await;
        }
    }
}
