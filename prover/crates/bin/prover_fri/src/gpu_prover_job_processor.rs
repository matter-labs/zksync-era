#[cfg(feature = "gpu")]
pub mod gpu_prover {
    use std::{collections::HashMap, sync::Arc, time::Instant};

    use anyhow::Context as _;
    use shivini::{
        gpu_proof_config::GpuProofConfig, gpu_prove_from_external_witness_data, ProverContext,
        ProverContextConfig,
    };
    use tokio::task::JoinHandle;
    use zksync_config::configs::{fri_prover_group::FriProverGroupConfig, FriProverConfig};
    use zksync_env_config::FromEnv;
    use zksync_object_store::ObjectStore;
    use zksync_prover_dal::{ConnectionPool, ProverDal};
    use zksync_prover_fri_types::{
        circuit_definitions::{
            base_layer_proof_config,
            boojum::{
                algebraic_props::{
                    round_function::AbsorptionModeOverwrite, sponge::GoldilocksPoseidon2Sponge,
                },
                cs::implementations::{pow::NoPow, transcript::GoldilocksPoisedon2Transcript},
                worker::Worker,
            },
            circuit_definitions::{
                base_layer::ZkSyncBaseLayerProof, recursion_layer::ZkSyncRecursionLayerProof,
            },
            recursion_layer_proof_config,
        },
        CircuitWrapper, FriProofWrapper, ProverServiceDataKey, WitnessVectorArtifacts,
    };
    use zksync_prover_fri_utils::region_fetcher::Zone;
    use zksync_queued_job_processor::{async_trait, JobProcessor};
    use zksync_types::{
        basic_fri_types::CircuitIdRoundTuple, protocol_version::ProtocolSemanticVersion,
        prover_dal::SocketAddress,
    };
    use zksync_vk_setup_data_server_fri::{keystore::Keystore, GoldilocksGpuProverSetupData};

    use crate::{
        metrics::METRICS,
        utils::{
            get_setup_data_key, save_proof, setup_metadata_to_setup_data_key, verify_proof,
            GpuProverJob, ProverArtifacts, SharedWitnessVectorQueue,
        },
    };

    type DefaultTranscript = GoldilocksPoisedon2Transcript;
    type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

    pub enum SetupLoadMode {
        FromMemory(HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>),
        FromDisk,
    }

    #[allow(dead_code)]
    pub struct Prover {
        blob_store: Arc<dyn ObjectStore>,
        public_blob_store: Option<Arc<dyn ObjectStore>>,
        config: Arc<FriProverConfig>,
        prover_connection_pool: ConnectionPool<zksync_prover_dal::Prover>,
        setup_load_mode: SetupLoadMode,
        // Only pick jobs for the configured circuit id and aggregation rounds.
        // Empty means all jobs are picked.
        circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
        witness_vector_queue: SharedWitnessVectorQueue,
        prover_context: ProverContext,
        address: SocketAddress,
        zone: Zone,
        protocol_version: ProtocolSemanticVersion,
    }

    impl Prover {
        #[allow(dead_code)]
        pub fn new(
            blob_store: Arc<dyn ObjectStore>,
            public_blob_store: Option<Arc<dyn ObjectStore>>,
            config: FriProverConfig,
            prover_connection_pool: ConnectionPool<zksync_prover_dal::Prover>,
            setup_load_mode: SetupLoadMode,
            circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
            witness_vector_queue: SharedWitnessVectorQueue,
            address: SocketAddress,
            zone: Zone,
            protocol_version: ProtocolSemanticVersion,
            max_allocation: Option<usize>,
        ) -> Self {
            let prover_context = match max_allocation {
                Some(max_allocation) => ProverContext::create_with_config(
                    ProverContextConfig::default().with_maximum_device_allocation(max_allocation),
                )
                .expect("failed initializing gpu prover context"),
                None => ProverContext::default().expect("failed initializing gpu prover context"),
            };
            Prover {
                blob_store,
                public_blob_store,
                config: Arc::new(config),
                prover_connection_pool,
                setup_load_mode,
                circuit_ids_for_round_to_be_proven,
                witness_vector_queue,
                prover_context,
                address,
                zone,
                protocol_version,
            }
        }

        #[tracing::instrument(name = "Prover::get_setup_data", skip_all)]
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
                    let keystore =
                        Keystore::new_with_setup_data_path(self.config.setup_data_path.clone());
                    let artifact: GoldilocksGpuProverSetupData = keystore
                        .load_gpu_setup_data_for_circuit_type(key.clone())
                        .context("load_gpu_setup_data_for_circuit_type()")?;

                    METRICS.gpu_setup_data_load_time[&key.circuit_id.to_string()]
                        .observe(started_at.elapsed());

                    Arc::new(artifact)
                }
            })
        }

        #[tracing::instrument(
            name = "Prover::prove",
            skip_all,
            fields(l1_batch = %job.witness_vector_artifacts.prover_job.block_number)
        )]
        pub fn prove(
            job: GpuProverJob,
            setup_data: Arc<GoldilocksGpuProverSetupData>,
        ) -> ProverArtifacts {
            let worker = Worker::new();
            let GpuProverJob {
                witness_vector_artifacts,
            } = job;
            let WitnessVectorArtifacts {
                witness_vector,
                prover_job,
            } = witness_vector_artifacts;

            let (gpu_proof_config, proof_config, circuit_id) = match &prover_job.circuit_wrapper {
                CircuitWrapper::Base(circuit) => (
                    GpuProofConfig::from_base_layer_circuit(circuit),
                    base_layer_proof_config(),
                    circuit.numeric_circuit_type(),
                ),
                CircuitWrapper::Recursive(circuit) => (
                    GpuProofConfig::from_recursive_layer_circuit(circuit),
                    recursion_layer_proof_config(),
                    circuit.numeric_circuit_type(),
                ),
                CircuitWrapper::BasePartial(_) => panic!("Invalid CircuitWrapper received"),
            };

            let started_at = Instant::now();
            let proof = gpu_prove_from_external_witness_data::<
                DefaultTranscript,
                DefaultTreeHasher,
                NoPow,
                _,
            >(
                &gpu_proof_config,
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
            METRICS.gpu_proof_generation_time[&circuit_id.to_string()]
                .observe(started_at.elapsed());

            let proof = proof.into();
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
                CircuitWrapper::BasePartial(_) => panic!("Received partial base circuit"),
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
            let now = Instant::now();
            tracing::info!("Attempting to get new job from assembly queue.");
            let mut queue = self.witness_vector_queue.lock().await;
            let is_full = queue.is_full();
            tracing::info!(
                "Queue has {} items with max capacity {}. Queue is_full = {}.",
                queue.size(),
                queue.capacity(),
                is_full
            );
            match queue.remove() {
                Err(_) => {
                    tracing::warn!("No assembly available in queue after {:?}.", now.elapsed());
                    Ok(None)
                }
                Ok(item) => {
                    if is_full {
                        self.prover_connection_pool
                            .connection()
                            .await
                            .unwrap()
                            .fri_gpu_prover_queue_dal()
                            .update_prover_instance_from_full_to_available(
                                self.address.clone(),
                                self.zone.to_string(),
                            )
                            .await;
                    }
                    tracing::info!(
                        "Assembly received after {:?}. Starting GPU proving for job: {:?}",
                        now.elapsed(),
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
                .connection()
                .await
                .unwrap()
                .fri_prover_jobs_dal()
                .save_proof_error(job_id, error)
                .await;
        }

        async fn process_job(
            &self,
            _job_id: &Self::JobId,
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
                let block_number = job.witness_vector_artifacts.prover_job.block_number;
                let _span = tracing::info_span!("gpu_prove", %block_number).entered();
                Ok(Self::prove(job, setup_data.context("get_setup_data()")?))
            })
        }

        async fn save_result(
            &self,
            job_id: Self::JobId,
            started_at: Instant,
            artifacts: Self::JobArtifacts,
        ) -> anyhow::Result<()> {
            METRICS.gpu_total_proving_time.observe(started_at.elapsed());

            let mut storage_processor = self.prover_connection_pool.connection().await.unwrap();
            save_proof(
                job_id,
                started_at,
                artifacts,
                &*self.blob_store,
                self.public_blob_store.as_deref(),
                self.config.shall_save_to_public_bucket,
                &mut storage_processor,
                self.protocol_version,
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
                .connection()
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
                let keystore = Keystore::new_with_setup_data_path(config.setup_data_path.clone());
                for prover_setup_metadata in prover_setup_metadata_list {
                    let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
                    let setup_data = keystore
                        .load_gpu_setup_data_for_circuit_type(key.clone())
                        .context("load_gpu_setup_data_for_circuit_type()")?;
                    cache.insert(key, Arc::new(setup_data));
                }
                SetupLoadMode::FromMemory(cache)
            }
        })
    }
}
