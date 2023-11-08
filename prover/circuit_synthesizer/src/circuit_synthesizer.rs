use std::option::Option;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context as _;
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

use zksync_env_config::FromEnv;

use zksync_object_store::{CircuitKey, ObjectStore, ObjectStoreError, ObjectStoreFactory};
use zksync_prover_dal::ProverConnectionPool;
use zksync_prover_fri_utils::socket_utils::send_assembly;
use zksync_prover_utils::numeric_index_to_circuit_name;
use zksync_prover_utils::region_fetcher::{get_region, get_zone};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{
    proofs::{GpuProverInstanceStatus, SocketAddress},
    protocol_version::L1VerifierConfig,
};

#[derive(thiserror::Error, Debug)]
pub enum CircuitSynthesizerError {
    #[error("InvalidaGroupCircuits: {0}")]
    InvalidGroupCircuits(u8),
    #[error("InvalidCircuitId: {0}")]
    InvalidCircuitId(u8),
    #[error("InputLoadFailed: {0}")]
    InputLoadFailed(ObjectStoreError),
    #[error("GetRegionFailed: {0}")]
    GetRegionFailed(anyhow::Error),
    #[error("GetZoneFailed: {0}")]
    GetZoneFailed(anyhow::Error),
}

pub struct CircuitSynthesizer {
    config: CircuitSynthesizerConfig,
    blob_store: Box<dyn ObjectStore>,
    allowed_circuit_types: Option<Vec<String>>,
    region: String,
    zone: String,
    vk_commitments: L1VerifierConfig,
    prover_connection_pool: ProverConnectionPool,
    main_connection_pool: ServerConnectionPool,
}

impl CircuitSynthesizer {
    pub async fn new(
        config: CircuitSynthesizerConfig,
        prover_groups: ProverGroupConfig,
        store_factory: &ObjectStoreFactory,
        vk_commitments: L1VerifierConfig,
        prover_connection_pool: ProverConnectionPool,
        main_connection_pool: ServerConnectionPool,
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

        tracing::info!(
            "Configured for group [{}], circuits: {allowed_circuit_types:?}",
            config.prover_group_id
        );

        Ok(Self {
            config,
            blob_store: store_factory.create_store().await,
            allowed_circuit_types: allowed_circuit_types
                .map(|x| x.into_iter().map(|x| x.1).collect()),
            region: get_region(&prover_groups)
                .await
                .map_err(CircuitSynthesizerError::GetRegionFailed)?,
            zone: get_zone(&prover_groups)
                .await
                .map_err(CircuitSynthesizerError::GetZoneFailed)?,
            vk_commitments,
            prover_connection_pool,
            main_connection_pool,
        })
    }

    pub fn synthesize(
        circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>,
    ) -> anyhow::Result<(ProvingAssembly, u8)> {
        let start_instant = Instant::now();

        let mut assembly = Prover::new_proving_assembly();
        circuit
            .synthesize(&mut assembly)
            .context("circuit synthesize failed")?;

        let circuit_type = numeric_index_to_circuit_name(circuit.numeric_circuit_type()).unwrap();

        tracing::info!(
            "Finished circuit synthesis for circuit: {circuit_type} took {:?}",
            start_instant.elapsed()
        );
        metrics::histogram!(
            "server.circuit_synthesizer.synthesize",
            start_instant.elapsed(),
            "circuit_type" => circuit_type,
        );

        // we don't perform assembly finalization here since it increases the assembly size significantly due to padding.
        Ok((assembly, circuit.numeric_circuit_type()))
    }
}

#[async_trait]
impl JobProcessor for CircuitSynthesizer {
    type Job = ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>;
    type JobId = u32;
    type JobArtifacts = (ProvingAssembly, u8);
    const SERVICE_NAME: &'static str = "CircuitSynthesizer";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        tracing::trace!(
            "Attempting to fetch job types: {:?}",
            self.allowed_circuit_types
        );
        let mut storage = self.prover_connection_pool.access_storage().await.unwrap();
        let protocol_versions = storage
            .prover_protocol_versions_dal()
            .protocol_version_for(&self.vk_commitments)
            .await;

        let prover_job = match &self.allowed_circuit_types {
            Some(types) => {
                storage
                    .prover_dal()
                    .get_next_prover_job_by_circuit_types(types.clone(), &protocol_versions)
                    .await
            }
            None => {
                storage
                    .prover_dal()
                    .get_next_prover_job(&protocol_versions)
                    .await
            }
        };
        let Some(prover_job) = prover_job else {
            return Ok(None);
        };

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
            .map_err(CircuitSynthesizerError::InputLoadFailed)?;

        Ok(Some((prover_job.id, input)))
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        let res = self
            .prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .prover_dal()
            .save_proof_error(job_id, error)
            .await;
        if let Err(err) = res {
            tracing::error!("save_proof_error(): {err:#}");
        } else {
            let attempts = res.0;
            let l1_batch_number = res.1;
            if attempts >= self.config.max_attempts {
                self.main_connection_pool
                    .access_storage()
                    .await
                    .unwrap()
                    .blocks_dal()
                    .set_skip_proof_for_l1_batch(L1BatchNumber(l1_batch_number))
                    .await
                    .unwrap();
            }
        }
    }

    async fn process_job(
        &self,
        job: Self::Job,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        tokio::task::spawn_blocking(move || Self::synthesize(job))
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        _started_at: Instant,
        (assembly, circuit_id): Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        tracing::trace!(
            "Finished circuit synthesis for job: {job_id} in region: {}",
            self.region
        );

        let now = Instant::now();
        let mut serialized: Vec<u8> = vec![];
        serialize_job(&assembly, job_id as usize, circuit_id, &mut serialized);

        tracing::trace!(
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
                .unwrap()
                .gpu_prover_queue_dal()
                .lock_available_prover(
                    self.config.gpu_prover_queue_timeout(),
                    self.config.prover_group_id,
                    self.region.clone(),
                    self.zone.clone(),
                )
                .await;

            if let Some(address) = prover {
                let result = send_assembly(job_id, &serialized, &address);
                handle_send_result(
                    &result,
                    job_id,
                    &address,
                    &self.prover_connection_pool,
                    self.region.clone(),
                    self.zone.clone(),
                )
                .await
                .context("handle_send_result()")?;

                if result.is_ok() {
                    return Ok(());
                }
                // We'll retry with another prover again, no point in dropping the results.

                tracing::warn!(
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
        tracing::trace!(
            "Not able to get any free prover instance for sending assembly for job: {job_id}"
        );
        Ok(())
    }
}

async fn handle_send_result(
    result: &Result<(Duration, u64), String>,
    job_id: u32,
    address: &SocketAddress,
    pool: &ProverConnectionPool,
    region: String,
    zone: String,
) -> anyhow::Result<()> {
    match result {
        Ok((elapsed, len)) => {
            let local_ip = local_ip().context("Failed obtaining local IP address")?;
            let blob_size_in_gb = len / (1024 * 1024 * 1024);

            // region: logs

            tracing::trace!(
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
                .unwrap()
                .prover_dal()
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
                .gpu_prover_queue_dal()
                .update_prover_instance_status(
                    address.clone(),
                    GpuProverInstanceStatus::Dead,
                    0,
                    region,
                    zone,
                )
                .await;

            let prover_config = ProverConfigs::from_env()
                .context("ProverConfigs::from_env()")?
                .non_gpu;
            // mark the job as failed
            let res = pool
                .access_storage()
                .await
                .unwrap()
                .prover_dal()
                .save_proof_error(job_id, "prover instance unreachable".to_string())
                .await;
            if let Err(err) = res {
                tracing::error!("save_proof_error(): {err}");
            } else {
                let attempts = res.0;
                let l1_batch_number = res.1;
                if attempts >= self.config.max_attempts {
                    self.main_connection_pool
                        .access_storage()
                        .await
                        .unwrap()
                        .blocks_dal()
                        .set_skip_proof_for_l1_batch(L1BatchNumber(l1_batch_number))
                        .await
                        .unwrap();
                }
            }
        }
    }
    Ok(())
}
