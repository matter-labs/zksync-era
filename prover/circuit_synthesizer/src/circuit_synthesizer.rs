use std::option::Option;
use std::time::Duration;
use std::time::Instant;

use local_ip_address::local_ip;
use prover_service::prover::{Prover, ProvingAssembly};
use prover_service::remote_synth::serialize_job;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::plonk::better_better_cs::cs::Circuit;
use zkevm_test_harness::pairing::bn256::Bn256;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;

use zksync_config::configs::prover_group::ProverGroupConfig;
use zksync_config::configs::CircuitSynthesizerConfig;
use zksync_config::ProverConfigs;
use zksync_dal::ConnectionPool;
use zksync_object_store::{CircuitKey, ObjectStore, ObjectStoreError, ObjectStoreFactory};
use zksync_prover_fri_utils::socket_utils::send_assembly;
use zksync_prover_utils::numeric_index_to_circuit_name;
use zksync_prover_utils::region_fetcher::{get_region, get_zone};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{protocol_version::L1VerifierConfig, proofs::{GpuProverInstanceStatus, SocketAddress}};

#[derive(Debug)]
pub enum CircuitSynthesizerError {
    InvalidGroupCircuits(u8),
    InvalidCircuitId(u8),
    InputLoadFailed(ObjectStoreError),
}

pub struct CircuitSynthesizer {
    config: CircuitSynthesizerConfig,
    blob_store: Box<dyn ObjectStore>,
    allowed_circuit_types: Option<Vec<String>>,
    region: String,
    zone: String,
    vk_commitments: L1VerifierConfig,
    prover_connection_pool: ConnectionPool,
}

impl CircuitSynthesizer {
    pub async fn new(
        config: CircuitSynthesizerConfig,
        prover_groups: ProverGroupConfig,
        store_factory: &ObjectStoreFactory,
        vk_commitments: L1VerifierConfig,
        prover_connection_pool: ConnectionPool,
    ) -> Result<Self, CircuitSynthesizerError> {
        let is_specialized = prover_groups.is_specialized_group_id(config.prover_group_id);
        let allowed_circuit_types = if is_specialized {
            let types = prover_groups
                .get_circuit_ids_for_group_id(config.prover_group_id)
                .ok_or(CircuitSynthesizerError::InvalidGroupCircuits(
                    config.prover_group_id,
                ))?
                .into_iter()
                .map(|id| {
                    numeric_index_to_circuit_name(id)
                        .map(|x| (id, x.to_owned()))
                        .ok_or(CircuitSynthesizerError::InvalidCircuitId(id))
                })
                .collect::<Result<Vec<_>, CircuitSynthesizerError>>()?;
            Some(types)
        } else {
            None
        };

        vlog::info!(
            "Configured for group [{}], circuits: {allowed_circuit_types:?}",
            config.prover_group_id
        );

        Ok(Self {
            config,
            blob_store: store_factory.create_store().await,
            allowed_circuit_types: allowed_circuit_types
                .map(|x| x.into_iter().map(|x| x.1).collect()),
            region: get_region().await,
            zone: get_zone().await,
            vk_commitments,
            prover_connection_pool,
        })
    }

    pub fn synthesize(
        circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>,
    ) -> (ProvingAssembly, u8) {
        let start_instant = Instant::now();

        let mut assembly = Prover::new_proving_assembly();
        circuit
            .synthesize(&mut assembly)
            .expect("circuit synthesize failed");

        let circuit_type = numeric_index_to_circuit_name(circuit.numeric_circuit_type()).unwrap();

        vlog::info!(
            "Finished circuit synthesis for circuit: {circuit_type} took {:?}",
            start_instant.elapsed()
        );
        metrics::histogram!(
            "server.circuit_synthesizer.synthesize",
            start_instant.elapsed(),
            "circuit_type" => circuit_type,
        );

        // we don't perform assembly finalization here since it increases the assembly size significantly due to padding.
        (assembly, circuit.numeric_circuit_type())
    }
}

#[async_trait]
impl JobProcessor for CircuitSynthesizer {
    type Job = ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>;
    type JobId = u32;
    type JobArtifacts = (ProvingAssembly, u8);
    const SERVICE_NAME: &'static str = "CircuitSynthesizer";

    async fn get_next_job(&self) -> Option<(Self::JobId, Self::Job)> {
        vlog::trace!(
            "Attempting to fetch job types: {:?}",
            self.allowed_circuit_types
        );
        let mut storage = self.prover_connection_pool.access_storage().await;
        let protocol_versions = storage
            .protocol_versions_dal()
            .protocol_version_for(&self.vk_commitments)
            .await;

        let prover_job = match &self.allowed_circuit_types {
            Some(types) => {
                storage
                    .prover_dal()
                    .get_next_prover_job_by_circuit_types(types.clone(), &protocol_versions)
                    .await
            }
            None => storage.prover_dal().get_next_prover_job(&protocol_versions).await,
        }?;

        let circuit_key = CircuitKey {
            block_number: prover_job.block_number,
            sequence_number: prover_job.sequence_number,
            circuit_type: &prover_job.circuit_type,
            aggregation_round: prover_job.aggregation_round,
        };
        let input = self
            .blob_store
            .get(circuit_key)
            .await
            .map_err(CircuitSynthesizerError::InputLoadFailed)
            .unwrap_or_else(|err| panic!("{err:?}"));

        Some((prover_job.id, input))
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        self.prover_connection_pool
            .access_storage()
            .await
            .prover_dal()
            .save_proof_error(job_id, error, self.config.max_attempts)
            .await;
    }

    async fn process_job(
        &self,
        job: Self::Job,
        _started_at: Instant,
    ) -> JoinHandle<Self::JobArtifacts> {
        tokio::task::spawn_blocking(move || Self::synthesize(job))
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        _started_at: Instant,
        (assembly, circuit_id): Self::JobArtifacts,
    ) {
        vlog::trace!(
            "Finished circuit synthesis for job: {job_id} in region: {}",
            self.region
        );

        let now = Instant::now();
        let mut serialized: Vec<u8> = vec![];
        serialize_job(&assembly, job_id as usize, circuit_id, &mut serialized);

        vlog::trace!(
            "Serialized circuit assembly for job {job_id} in {:?}",
            now.elapsed()
        );

        let now = Instant::now();
        let mut attempts = 0;

        while now.elapsed() < self.config.prover_instance_wait_timeout() {
            let prover = self
                .prover_connection_pool
                .access_storage()
                .await
                .gpu_prover_queue_dal()
                .lock_available_prover(
                    self.config.gpu_prover_queue_timeout(),
                    self.config.prover_group_id,
                    self.region.clone(),
                    self.zone.clone(),
                )
                .await;

            if let Some(address) = prover {
                let result = send_assembly(job_id, &mut serialized, &address);
                handle_send_result(
                    &result,
                    job_id,
                    &address,
                    &self.prover_connection_pool,
                    self.region.clone(),
                    self.zone.clone(),
                )
                    .await;

                if result.is_ok() {
                    return;
                }
                // We'll retry with another prover again, no point in dropping the results.

                vlog::warn!(
                    "Could not send assembly to {address:?}. Prover group {}, region {}, \
                         circuit id {circuit_id}, send attempt {attempts}.",
                    self.config.prover_group_id,
                    self.region
                );
                attempts += 1;
            } else {
                sleep(self.config.prover_instance_poll_time()).await;
            }
        }
        vlog::trace!(
            "Not able to get any free prover instance for sending assembly for job: {job_id}"
        );
    }
}

async fn handle_send_result(
    result: &Result<(Duration, u64), String>,
    job_id: u32,
    address: &SocketAddress,
    pool: &ConnectionPool,
    region: String,
    zone: String,
) {
    match result {
        Ok((elapsed, len)) => {
            let local_ip = local_ip().expect("Failed obtaining local IP address");
            let blob_size_in_gb = len / (1024 * 1024 * 1024);

            // region: logs

            vlog::trace!(
                "Sent assembly of size: {blob_size_in_gb}GB successfully, took: {elapsed:?} \
                 for job: {job_id} by: {local_ip:?} to: {address:?}"
            );
            metrics::histogram!(
                "server.circuit_synthesizer.blob_sending_time",
                *elapsed,
                "blob_size_in_gb" => blob_size_in_gb.to_string(),
            );

            // endregion

            pool.access_storage()
                .await
                .prover_dal()
                .update_status(job_id, "in_gpu_proof")
                .await;
        }

        Err(err) => {
            vlog::trace!(
                "Failed sending assembly to address: {address:?}, socket not reachable \
                 reason: {err}"
            );

            // mark prover instance in gpu_prover_queue dead
            pool.access_storage()
                .await
                .gpu_prover_queue_dal()
                .update_prover_instance_status(
                    address.clone(),
                    GpuProverInstanceStatus::Dead,
                    0,
                    region,
                    zone,
                )
                .await;

            let prover_config = ProverConfigs::from_env().non_gpu;
            // mark the job as failed
            pool.access_storage()
                .await
                .prover_dal()
                .save_proof_error(
                    job_id,
                    "prover instance unreachable".to_string(),
                    prover_config.max_attempts,
                )
                .await;
        }
    }
}
