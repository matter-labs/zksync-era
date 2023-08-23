#[cfg(feature = "gpu")]
pub mod gpu_prover {
    use std::collections::HashMap;
    use std::{sync::Arc, time::Instant};

    use queues::IsQueue;
    use tokio::task::JoinHandle;
    use zksync_prover_fri_types::circuit_definitions::base_layer_proof_config;
    use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::round_function::AbsorptionModeOverwrite;
    use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::sponge::GoldilocksPoseidon2Sponge;
    use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::pow::NoPow;
    use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver;
    use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::transcript::GoldilocksPoisedon2Transcript;
    use zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker;
    use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerProof;
    use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof;
    use zksync_vk_setup_data_server_fri::GpuProverSetupData;

    use zksync_config::configs::fri_prover_group::{CircuitIdRoundTuple, FriProverGroupConfig};
    use zksync_config::configs::FriProverConfig;
    use zksync_dal::ConnectionPool;
    use zksync_object_store::ObjectStore;
    use zksync_prover_fri_types::{
        CircuitWrapper, FriProofWrapper, ProverServiceDataKey, WitnessVectorArtifacts,
    };
    use zksync_queued_job_processor::{async_trait, JobProcessor};
    use zksync_vk_setup_data_server_fri::get_setup_data_for_circuit_type;
    use {
        shivini::gpu_prove_from_external_witness_data,
        shivini::synthesis_utils::init_base_layer_cs_for_repeated_proving,
        shivini::synthesis_utils::init_recursive_layer_cs_for_repeated_proving,
        shivini::ProverContext, zksync_vk_setup_data_server_fri::GoldilocksGpuProverSetupData,
    };

    use crate::utils::{
        save_proof, setup_metadata_to_setup_data_key, verify_proof, ProverArtifacts,
        SharedWitnessVectorQueue, F, H,
    };

    type DefaultTranscript = GoldilocksPoisedon2Transcript;
    type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

    pub enum SetupLoadMode {
        FromMemory(HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>),
        FromDisk,
    }

    pub struct Prover {
        blob_store: Box<dyn ObjectStore>,
        public_blob_store: Box<dyn ObjectStore>,
        config: Arc<FriProverConfig>,
        prover_connection_pool: ConnectionPool,
        setup_load_mode: SetupLoadMode,
        // Only pick jobs for the configured circuit id and aggregation rounds.
        // Empty means all jobs are picked.
        circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
        witness_vector_queue: SharedWitnessVectorQueue,
        prover_context: ProverContext,
    }

    impl Prover {
        pub fn new(
            blob_store: Box<dyn ObjectStore>,
            public_blob_store: Box<dyn ObjectStore>,
            config: FriProverConfig,
            prover_connection_pool: ConnectionPool,
            setup_load_mode: SetupLoadMode,
            circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
            witness_vector_queue: SharedWitnessVectorQueue,
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
            }
        }

        fn get_setup_data(&self, key: ProverServiceDataKey) -> Arc<GoldilocksGpuProverSetupData> {
            match &self.setup_load_mode {
                SetupLoadMode::FromMemory(cache) => cache
                    .get(&key)
                    .expect("Setup data not found in cache")
                    .clone(),
                SetupLoadMode::FromDisk => {
                    let started_at = Instant::now();
                    let artifact: GoldilocksGpuProverSetupData =
                        get_setup_data_for_circuit_type(key.clone());
                    metrics::histogram!(
                        "prover_fri.prover.gpu_setup_data_load_time",
                        started_at.elapsed(),
                        "circuit_type" => key.circuit_id.to_string(),
                    );
                    Arc::new(artifact)
                }
            }
        }

        pub fn prove(
            job: WitnessVectorArtifacts,
            config: Arc<FriProverConfig>,
            setup_data: Arc<GoldilocksGpuProverSetupData>,
        ) -> ProverArtifacts {
            let worker = Worker::new();
            let started_at = Instant::now();
            let (cs, proof_config, circuit_id) = match job.prover_job.circuit_wrapper.clone() {
                CircuitWrapper::Base(base_circuit) => {
                    let circuit_id = base_circuit.numeric_circuit_type();
                    let cs = init_base_layer_cs_for_repeated_proving(
                        base_circuit,
                        &setup_data.finalization_hint,
                    );
                    (cs, base_layer_proof_config(), circuit_id)
                }
                CircuitWrapper::Recursive(recursive_circuit) => {
                    let circuit_id = recursive_circuit.numeric_circuit_type();
                    let cs = init_recursive_layer_cs_for_repeated_proving(
                        recursive_circuit,
                        &setup_data.finalization_hint,
                    );
                    (cs, base_layer_proof_config(), circuit_id)
                }
            };
            vlog::info!(
                "Successfully generated assembly without witness vector for job: {}, took: {:?}",
                job.prover_job.job_id,
                started_at.elapsed()
            );
            metrics::histogram!(
                "prover_fri.prover.gpu_assembly_generation_time",
                started_at.elapsed(),
                "circuit_type" => circuit_id.to_string()
            );
            let started_at = Instant::now();
            let proof = gpu_prove_from_external_witness_data::<
                _,
                DefaultTranscript,
                DefaultTreeHasher,
                NoPow,
                _,
            >(
                cs,
                &job.witness_vector,
                proof_config,
                &setup_data.setup,
                &setup_data.vk,
                (),
                &worker,
            )
            .unwrap_or_else(|_| {
                panic!(
                    "failed generating GPU proof for id: {}",
                    job.prover_job.job_id
                )
            });
            metrics::histogram!(
                "prover_fri.prover.gpu_proof_generation_time",
                started_at.elapsed(),
                "circuit_type" => circuit_id.to_string()
            );
            verify_proof(
                &job.prover_job.circuit_wrapper,
                &proof,
                &setup_data.vk,
                job.prover_job.job_id,
            );
            let proof_wrapper = match &job.prover_job.circuit_wrapper {
                CircuitWrapper::Base(_) => {
                    FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
                }
                CircuitWrapper::Recursive(circuit) => FriProofWrapper::Recursive(
                    ZkSyncRecursionLayerProof::from_inner(circuit_id, proof),
                ),
            };
            ProverArtifacts::new(job.prover_job.block_number, proof_wrapper)
        }
    }

    #[async_trait]
    impl JobProcessor for Prover {
        type Job = WitnessVectorArtifacts;
        type JobId = u32;
        type JobArtifacts = ProverArtifacts;

        // we use smaller number here as the polling in done from the in-memory queue not DB
        const POLLING_INTERVAL_MS: u64 = 200;
        const MAX_BACKOFF_MS: u64 = 1_000;
        const SERVICE_NAME: &'static str = "FriGpuProver";

        async fn get_next_job(&self) -> Option<(Self::JobId, Self::Job)> {
            let mut queue = self.witness_vector_queue.lock().await;
            match queue.remove() {
                Err(_) => None,
                Ok(item) => {
                    vlog::info!("Started GPU proving for job: {:?}", item.prover_job.job_id);
                    Some((item.prover_job.job_id, item))
                }
            }
        }

        async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
            self.prover_connection_pool
                .access_storage()
                .await
                .fri_prover_jobs_dal()
                .save_proof_error(job_id, error)
                .await;
        }

        async fn process_job(
            &self,
            job: Self::Job,
            _started_at: Instant,
        ) -> JoinHandle<Self::JobArtifacts> {
            let config = Arc::clone(&self.config);
            let setup_data = self.get_setup_data(job.prover_job.setup_data_key.clone());
            tokio::task::spawn_blocking(move || Self::prove(job, config, setup_data))
        }

        async fn save_result(
            &self,
            job_id: Self::JobId,
            started_at: Instant,
            artifacts: Self::JobArtifacts,
        ) {
            metrics::histogram!(
                "prover_fri.prover.gpu_total_proving_time",
                started_at.elapsed(),
            );
            let mut storage_processor = self.prover_connection_pool.access_storage().await;
            save_proof(
                job_id,
                started_at,
                artifacts,
                &*self.blob_store,
                &*self.public_blob_store,
                &mut storage_processor,
            )
            .await;
        }
    }

    pub fn load_setup_data_cache(config: &FriProverConfig) -> SetupLoadMode {
        match config.setup_load_mode {
            zksync_config::configs::fri_prover::SetupLoadMode::FromDisk => SetupLoadMode::FromDisk,
            zksync_config::configs::fri_prover::SetupLoadMode::FromMemory => {
                let mut cache = HashMap::new();
                vlog::info!(
                    "Loading setup data cache for group {}",
                    &config.specialized_group_id
                );
                let prover_setup_metadata_list = FriProverGroupConfig::from_env()
                    .get_circuit_ids_for_group_id(config.specialized_group_id)
                    .expect(
                        "At least one circuit should be configured for group when running in FromMemory mode",
                    );
                vlog::info!(
                    "for group {} configured setup metadata are {:?}",
                    &config.specialized_group_id,
                    prover_setup_metadata_list
                );
                for prover_setup_metadata in prover_setup_metadata_list {
                    let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
                    let setup_data = get_setup_data_for_circuit_type(key.clone());
                    cache.insert(key, Arc::new(setup_data));
                }
                SetupLoadMode::FromMemory(cache)
            }
        }
    }

    pub fn init_finalization_hints_cache(
        config: &FriProverConfig,
    ) -> HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>> {
        let mut cache = HashMap::new();
        vlog::info!(
            "Loading finalization hint for group {}",
            &config.specialized_group_id
        );
        let group_config = FriProverGroupConfig::from_env();
        let prover_setup_metadata_list = match config.setup_load_mode {
            zksync_config::configs::fri_prover::SetupLoadMode::FromDisk => group_config.get_all_circuit_ids(),
            zksync_config::configs::fri_prover::SetupLoadMode::FromMemory =>
                group_config.get_circuit_ids_for_group_id(config.specialized_group_id).expect(
                        "At least one circuit should be configured for group when running in FromMemory mode")
        };

        for prover_setup_metadata in prover_setup_metadata_list {
            let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
            let setup_data: GpuProverSetupData<F, H> = get_setup_data_for_circuit_type(key.clone());
            cache.insert(key, Arc::new(setup_data.finalization_hint));
        }
        cache
    }
}
