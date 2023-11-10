#[cfg(feature = "gpu")]
pub mod gpu_prover {
    use std::collections::HashMap;
    use std::{sync::Arc, time::Instant};

    use anyhow::Context as _;
    use tokio::task::JoinHandle;
    use zksync_prover_fri_types::circuit_definitions::base_layer_proof_config;
    use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::round_function::AbsorptionModeOverwrite;
    use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::sponge::GoldilocksPoseidon2Sponge;
    use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::pow::NoPow;

    use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::transcript::GoldilocksPoisedon2Transcript;
    use zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker;
    use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerProof;
    use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof;
    use zksync_prover_fri_types::WitnessVectorArtifacts;

    use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
    use zksync_config::configs::FriProverConfig;
    use zksync_dal::ConnectionPool;
    use zksync_env_config::FromEnv;
    use zksync_object_store::ObjectStore;
    use zksync_prover_fri_types::{CircuitWrapper, FriProofWrapper, ProverServiceDataKey};
    use zksync_queued_job_processor::{async_trait, JobProcessor};
    use zksync_types::{basic_fri_types::CircuitIdRoundTuple, proofs::SocketAddress};
    use zksync_vk_setup_data_server_fri::get_setup_data_for_circuit_type;
    use {
        shivini::gpu_prove_from_external_witness_data, shivini::ProverContext,
        zksync_vk_setup_data_server_fri::GoldilocksGpuProverSetupData,
    };

    use crate::utils::{
        get_setup_data_key, save_proof, setup_metadata_to_setup_data_key, verify_proof,
        GpuProverJob, ProverArtifacts, SharedWitnessVectorQueue,
    };

    type DefaultTranscript = GoldilocksPoisedon2Transcript;
    type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

    pub enum SetupLoadMode {
        FromMemory(HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>),
        FromDisk,
    }

    #[allow(dead_code)]
    pub struct Prover {
        blob_store: Box<dyn ObjectStore>,
        public_blob_store: Option<Box<dyn ObjectStore>>,
        config: Arc<FriProverConfig>,
        prover_connection_pool: ConnectionPool,
        setup_load_mode: SetupLoadMode,
        // Only pick jobs for the configured circuit id and aggregation rounds.
        // Empty means all jobs are picked.
        circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
        witness_vector_queue: SharedWitnessVectorQueue,
        prover_context: ProverContext,
        address: SocketAddress,
        zone: String,
    }

    impl Prover {
        #[allow(dead_code)]
        pub fn new(
            blob_store: Box<dyn ObjectStore>,
            public_blob_store: Option<Box<dyn ObjectStore>>,
            config: FriProverConfig,
            prover_connection_pool: ConnectionPool,
            setup_load_mode: SetupLoadMode,
            circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
            witness_vector_queue: SharedWitnessVectorQueue,
            address: SocketAddress,
            zone: String,
        ) -> Self {
            Prover {
                blob_store,
                public_blob_store,
                config: Arc::new(config),
                prover_connection_pool,
                setup_load_mode,
                circuit_ids_for_round_to_be_proven,
                witness_vector_queue,
                prover_context: ProverContext::create()
                    .expect("failed initializing gpu prover context"),
                address,
                zone,
            }
        }

        fn get_setup_data(
            &self,
            key: ProverServiceDataKey,
        ) -> anyhow::Result<Arc<GoldilocksGpuProverSetupData>> {
            let key = get_setup_data_key(key);
            Ok(match &self.setup_load_mode {
                SetupLoadMode::FromMemory(cache) => cache
                    .get(&key)
                    .context("Setup data not found in cache")?
                    .clone(),
                SetupLoadMode::FromDisk => {
                    let started_at = Instant::now();
                    let artifact: GoldilocksGpuProverSetupData =
                        get_setup_data_for_circuit_type(key.clone())
                            .context("get_setup_data_for_circuit_type()")?;
                    metrics::histogram!(
                        "prover_fri.prover.gpu_setup_data_load_time",
                        started_at.elapsed(),
                        "circuit_type" => key.circuit_id.to_string(),
                    );
                    Arc::new(artifact)
                }
            })
        }

        pub fn prove(
            job: GpuProverJob,
            setup_data: Arc<GoldilocksGpuProverSetupData>,
        ) -> ProverArtifacts {
            let worker = Worker::new();
            let GpuProverJob {
                assembly,
                witness_vector_artifacts,
            } = job;
            let WitnessVectorArtifacts {
                witness_vector,
                prover_job,
            } = witness_vector_artifacts;

            let (proof_config, circuit_id) = match &prover_job.circuit_wrapper {
                CircuitWrapper::Base(base_circuit) => (
                    base_layer_proof_config(),
                    base_circuit.numeric_circuit_type(),
                ),
                CircuitWrapper::Recursive(recursive_circuit) => (
                    base_layer_proof_config(),
                    recursive_circuit.numeric_circuit_type(),
                ),
            };

            let started_at = Instant::now();
            let proof = gpu_prove_from_external_witness_data::<
                _,
                DefaultTranscript,
                DefaultTreeHasher,
                NoPow,
                _,
            >(
                assembly,
                &witness_vector,
                proof_config,
                &setup_data.setup,
                &setup_data.vk,
                (),
                &worker,
            )
            .unwrap_or_else(|_| {
                panic!("failed generating GPU proof for id: {}", prover_job.job_id)
            });
            tracing::info!(
                "Successfully generated gpu proof for job {} took: {:?}",
                prover_job.job_id,
                started_at.elapsed()
            );
            metrics::histogram!(
                "prover_fri.prover.gpu_proof_generation_time",
                started_at.elapsed(),
                "circuit_type" => circuit_id.to_string()
            );
            verify_proof(
                &prover_job.circuit_wrapper,
                &proof,
                &setup_data.vk,
                prover_job.job_id,
            );
            let proof_wrapper = match &prover_job.circuit_wrapper {
                CircuitWrapper::Base(_) => {
                    FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
                }
                CircuitWrapper::Recursive(_) => FriProofWrapper::Recursive(
                    ZkSyncRecursionLayerProof::from_inner(circuit_id, proof),
                ),
            };
            ProverArtifacts::new(prover_job.block_number, proof_wrapper)
        }
    }

    #[async_trait]
    impl JobProcessor for Prover {
        type Job = GpuProverJob;
        type JobId = u32;
        type JobArtifacts = ProverArtifacts;

        // we use smaller number here as the polling in done from the in-memory queue not DB
        const POLLING_INTERVAL_MS: u64 = 200;
        const MAX_BACKOFF_MS: u64 = 1_000;
        const SERVICE_NAME: &'static str = "FriGpuProver";

        async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
            let mut queue = self.witness_vector_queue.lock().await;
            let is_full = queue.is_full();
            match queue.remove() {
                Err(_) => Ok(None),
                Ok(item) => {
                    if is_full {
                        self.prover_connection_pool
                            .access_storage()
                            .await
                            .unwrap()
                            .fri_gpu_prover_queue_dal()
                            .update_prover_instance_from_full_to_available(
                                self.address.clone(),
                                self.zone.clone(),
                            )
                            .await;
                    }
                    tracing::info!(
                        "Started GPU proving for job: {:?}",
                        item.witness_vector_artifacts.prover_job.job_id
                    );
                    Ok(Some((
                        item.witness_vector_artifacts.prover_job.job_id,
                        item,
                    )))
                }
            }
        }

        async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
            self.prover_connection_pool
                .access_storage()
                .await
                .unwrap()
                .fri_prover_jobs_dal()
                .save_proof_error(job_id, error)
                .await;
        }

        async fn process_job(
            &self,
            job: Self::Job,
            _started_at: Instant,
        ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
            let setup_data = self.get_setup_data(
                job.witness_vector_artifacts
                    .prover_job
                    .setup_data_key
                    .clone(),
            );
            tokio::task::spawn_blocking(move || {
                Ok(Self::prove(job, setup_data.context("get_setup_data()")?))
            })
        }

        async fn save_result(
            &self,
            job_id: Self::JobId,
            started_at: Instant,
            artifacts: Self::JobArtifacts,
        ) -> anyhow::Result<()> {
            metrics::histogram!(
                "prover_fri.prover.gpu_total_proving_time",
                started_at.elapsed(),
            );
            let mut storage_processor = self.prover_connection_pool.access_storage().await.unwrap();
            save_proof(
                job_id,
                started_at,
                artifacts,
                &*self.blob_store,
                self.public_blob_store.as_deref(),
                self.config.shall_save_to_public_bucket,
                &mut storage_processor,
            )
            .await;
            Ok(())
        }

        fn max_attempts(&self) -> u32 {
            self.config.max_attempts
        }

        async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
            let mut prover_storage = self
                .prover_connection_pool
                .access_storage()
                .await
                .context("failed to acquire DB connection for Prover")?;
            prover_storage
                .fri_prover_jobs_dal()
                .get_prover_job_attempts(*job_id)
                .await
                .map(|attempts| attempts.unwrap_or(0))
                .context("failed to get job attempts for Prover")
        }
    }

    pub fn load_setup_data_cache(config: &FriProverConfig) -> anyhow::Result<SetupLoadMode> {
        Ok(match config.setup_load_mode {
            zksync_config::configs::fri_prover::SetupLoadMode::FromDisk => SetupLoadMode::FromDisk,
            zksync_config::configs::fri_prover::SetupLoadMode::FromMemory => {
                let mut cache = HashMap::new();
                tracing::info!(
                    "Loading setup data cache for group {}",
                    &config.specialized_group_id
                );
                let prover_setup_metadata_list = FriProverGroupConfig::from_env()
                    .context("FriProverGroupConfig::from_env()")?
                    .get_circuit_ids_for_group_id(config.specialized_group_id)
                    .context(
                        "At least one circuit should be configured for group when running in FromMemory mode",
                    )?;
                tracing::info!(
                    "for group {} configured setup metadata are {:?}",
                    &config.specialized_group_id,
                    prover_setup_metadata_list
                );
                for prover_setup_metadata in prover_setup_metadata_list {
                    let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
                    let setup_data = get_setup_data_for_circuit_type(key.clone())
                        .context("get_setup_data_for_circuit_type()")?;
                    cache.insert(key, Arc::new(setup_data));
                }
                SetupLoadMode::FromMemory(cache)
            }
        })
    }
}
