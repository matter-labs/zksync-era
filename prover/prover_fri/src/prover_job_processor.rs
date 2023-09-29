use std::collections::HashMap;
use std::{sync::Arc, time::Instant};
use tokio::task::JoinHandle;

use circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;

use circuit_definitions::boojum::cs::implementations::pow::NoPow;

use circuit_definitions::boojum::algebraic_props::round_function::AbsorptionModeOverwrite;
use circuit_definitions::boojum::algebraic_props::sponge::GoldilocksPoseidon2Sponge;
use circuit_definitions::boojum::cs::implementations::proof::Proof;
use circuit_definitions::boojum::cs::implementations::transcript::GoldilocksPoisedon2Transcript;
use circuit_definitions::boojum::cs::implementations::verifier::VerificationKey;
use circuit_definitions::boojum::field::goldilocks::GoldilocksExt2;
use circuit_definitions::boojum::worker::Worker;
use circuit_definitions::circuit_definitions::base_layer::{
    ZkSyncBaseLayerCircuit, ZkSyncBaseLayerProof,
};
use circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerProof, ZkSyncRecursiveLayerCircuit,
};
use circuit_definitions::{
    base_layer_proof_config, recursion_layer_proof_config, ZkSyncDefaultRoundFunction,
};

use zkevm_test_harness::boojum::field::goldilocks::GoldilocksField;
use zkevm_test_harness::prover_utils::{
    prove_base_layer_circuit, prove_recursion_layer_circuit, verify_base_layer_proof,
    verify_recursion_layer_proof,
};
use zksync_config::configs::FriProverConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{FriCircuitKey, ObjectStore};
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::L1BatchNumber;

use zksync_config::configs::fri_prover_group::CircuitIdRoundTuple;
use zksync_prover_fri_utils::{
    get_base_layer_circuit_id_for_recursive_layer, CircuitWrapper, FriProofWrapper,
};
use zksync_vk_setup_data_server_fri::{
    get_setup_data_for_circuit_type, GoldilocksProverSetupData, ProverServiceDataKey,
};
#[cfg(feature = "gpu")]
use {
    shivini::gpu_prove, shivini::synthesis_utils::synth_base_circuit_for_proving,
    shivini::synthesis_utils::synth_recursive_circuit_for_proving,
    shivini::ProverContext,
    zksync_vk_setup_data_server_fri::GoldilocksGpuProverSetupData,
};

type F = GoldilocksField;
type H = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
type EXT = GoldilocksExt2;
type DefaultTranscript = GoldilocksPoisedon2Transcript;
type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

#[cfg(not(feature = "gpu"))]
pub enum SetupLoadMode {
    FromMemory(HashMap<ProverServiceDataKey, Arc<GoldilocksProverSetupData>>),
    FromDisk,
}

#[cfg(feature = "gpu")]
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
    #[cfg(feature = "gpu")]
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
    ) -> Self {
        Prover {
            blob_store,
            public_blob_store,
            config: Arc::new(config),
            prover_connection_pool,
            setup_load_mode,
            circuit_ids_for_round_to_be_proven,
            #[cfg(feature = "gpu")]
            prover_context: ProverContext::create().expect("failed initializing gpu prover context"),
        }
    }

    #[cfg(not(feature = "gpu"))]
    fn get_setup_data(&self, key: ProverServiceDataKey) -> Arc<GoldilocksProverSetupData> {
        match &self.setup_load_mode {
            SetupLoadMode::FromMemory(cache) => cache
                .get(&key)
                .expect("Setup data not found in cache")
                .clone(),
            SetupLoadMode::FromDisk => {
                let started_at = Instant::now();
                let artifact: GoldilocksProverSetupData =
                    get_setup_data_for_circuit_type(key.clone());
                metrics::histogram!(
                    "prover_fri.prover.setup_data_load_time",
                    started_at.elapsed(),
                    "circuit_type" => key.circuit_id.to_string(),
                );
                Arc::new(artifact)
            }
        }
    }

    #[cfg(feature = "gpu")]
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

    #[cfg(feature = "gpu")]
    pub fn prove(
        job: ProverJob,
        config: Arc<FriProverConfig>,
        setup_data: Arc<GoldilocksGpuProverSetupData>,
    ) -> ProverArtifacts {
        let worker = Worker::new();
        let started_at = Instant::now();
        let (assembly, proof_config, circuit_id) = match job.circuit_wrapper.clone() {
            CircuitWrapper::Base(base_circuit) => {
                let circuit_id = base_circuit.numeric_circuit_type();
                (
                    synth_base_circuit_for_proving(base_circuit, &setup_data.finalization_hint),
                    base_layer_proof_config(),
                    circuit_id,
                )
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                let circuit_id = recursive_circuit.numeric_circuit_type();
                (
                    synth_recursive_circuit_for_proving(
                        recursive_circuit,
                        &setup_data.finalization_hint,
                    ),
                    recursion_layer_proof_config(),
                    circuit_id,
                )
            }
        };
        metrics::histogram!(
            "prover_fri.prover.gpu_circuit_synthesis_time",
            started_at.elapsed(),
            "circuit_type" => circuit_id.to_string()
        );
        let started_at = Instant::now();
        let proof = gpu_prove::<_, _, DefaultTranscript, DefaultTreeHasher, NoPow, _>(
            assembly,
            proof_config,
            &setup_data.setup,
            &setup_data.vk,
            &setup_data.vars_hint,
            &setup_data.wits_hint,
            (),
            &worker,
        )
        .unwrap_or_else(|_| panic!("failed generating GPU proof for id: {}", job.job_id));
        metrics::histogram!(
            "prover_fri.prover.gpu_proof_generation_time",
            started_at.elapsed(),
            "circuit_type" => circuit_id.to_string()
        );
        Self::verify_proof(&job.circuit_wrapper, &proof, &setup_data.vk, job.job_id);
        let proof_wrapper = match &job.circuit_wrapper {
            CircuitWrapper::Base(_) => {
                FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
            }
            CircuitWrapper::Recursive(circuit) => {
                FriProofWrapper::Recursive(ZkSyncRecursionLayerProof::from_inner(circuit_id, proof))
            }
        };
        ProverArtifacts::new(job.block_number, proof_wrapper)
    }

    #[cfg(not(feature = "gpu"))]
    pub fn prove(
        job: ProverJob,
        config: Arc<FriProverConfig>,
        setup_data: Arc<GoldilocksProverSetupData>,
    ) -> ProverArtifacts {
        let proof = match job.circuit_wrapper {
            CircuitWrapper::Base(base_circuit) => {
                Self::prove_base_layer(job.job_id, base_circuit, config, setup_data)
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                Self::prove_recursive_layer(job.job_id, recursive_circuit, config, setup_data)
            }
        };
        ProverArtifacts::new(job.block_number, proof)
    }

    fn prove_recursive_layer(
        job_id: u32,
        circuit: ZkSyncRecursiveLayerCircuit,
        config: Arc<FriProverConfig>,
        artifact: Arc<GoldilocksProverSetupData>,
    ) -> FriProofWrapper {
        let worker = Worker::new();
        let circuit_id = circuit.numeric_circuit_type();
        let started_at = Instant::now();
        let proof = prove_recursion_layer_circuit::<NoPow>(
            circuit.clone(),
            &worker,
            recursion_layer_proof_config(),
            &artifact.setup_base,
            &artifact.setup,
            &artifact.setup_tree,
            &artifact.vk,
            &artifact.vars_hint,
            &artifact.wits_hint,
            &artifact.finalization_hint,
        );
        metrics::histogram!(
            "prover_fri.prover.proof_generation_time",
            started_at.elapsed(),
            "circuit_type" => circuit_id.to_string(),
            "layer" => "recursive",
        );
        Self::verify_proof(
            &CircuitWrapper::Recursive(circuit),
            &proof,
            &artifact.vk,
            job_id,
        );
        FriProofWrapper::Recursive(ZkSyncRecursionLayerProof::from_inner(circuit_id, proof))
    }

    fn prove_base_layer(
        job_id: u32,
        circuit: ZkSyncBaseLayerCircuit<
            GoldilocksField,
            VmWitnessOracle<GoldilocksField>,
            ZkSyncDefaultRoundFunction,
        >,
        config: Arc<FriProverConfig>,
        artifact: Arc<GoldilocksProverSetupData>,
    ) -> FriProofWrapper {
        let worker = Worker::new();
        let circuit_id = circuit.numeric_circuit_type();
        let started_at = Instant::now();
        let proof = prove_base_layer_circuit::<NoPow>(
            circuit.clone(),
            &worker,
            base_layer_proof_config(),
            &artifact.setup_base,
            &artifact.setup,
            &artifact.setup_tree,
            &artifact.vk,
            &artifact.vars_hint,
            &artifact.wits_hint,
            &artifact.finalization_hint,
        );
        metrics::histogram!(
            "prover_fri.prover.proof_generation_time",
            started_at.elapsed(),
            "circuit_type" => circuit_id.to_string(),
            "layer" => "base",
        );
        Self::verify_proof(&CircuitWrapper::Base(circuit), &proof, &artifact.vk, job_id);
        FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
    }

    fn verify_proof(
        circuit_wrapper: &CircuitWrapper,
        proof: &Proof<F, H, EXT>,
        vk: &VerificationKey<F, H>,
        job_id: u32,
    ) {
        let started_at = Instant::now();
        let (is_valid, circuit_id) = match circuit_wrapper {
            CircuitWrapper::Base(base_circuit) => (
                verify_base_layer_proof::<NoPow>(&base_circuit, proof, vk),
                base_circuit.numeric_circuit_type(),
            ),
            CircuitWrapper::Recursive(recursive_circuit) => (
                verify_recursion_layer_proof::<NoPow>(&recursive_circuit, proof, vk),
                recursive_circuit.numeric_circuit_type(),
            ),
        };
        metrics::histogram!(
            "prover_fri.prover.proof_verification_time",
            started_at.elapsed(),
            "circuit_type" => circuit_id.to_string(),
        );
        if !is_valid {
            vlog::error!(
                "Failed to verify base layer proof for job-id: {} circuit_type {}",
                job_id,
                circuit_id
            );
        }
    }
}

pub struct ProverJob {
    block_number: L1BatchNumber,
    job_id: u32,
    circuit_wrapper: CircuitWrapper,
    setup_data_key: ProverServiceDataKey,
}

impl ProverJob {
    pub fn new(
        block_number: L1BatchNumber,
        job_id: u32,
        circuit_wrapper: CircuitWrapper,
        setup_data_key: ProverServiceDataKey,
    ) -> Self {
        Self {
            block_number,
            job_id,
            circuit_wrapper,
            setup_data_key,
        }
    }
}

pub struct ProverArtifacts {
    block_number: L1BatchNumber,
    pub proof_wrapper: FriProofWrapper,
}

impl ProverArtifacts {
    fn new(block_number: L1BatchNumber, proof_wrapper: FriProofWrapper) -> Self {
        Self {
            block_number,
            proof_wrapper,
        }
    }
}

#[async_trait]
impl JobProcessor for Prover {
    type Job = ProverJob;
    type JobId = u32;
    type JobArtifacts = ProverArtifacts;
    const SERVICE_NAME: &'static str = "FriProver";

    async fn get_next_job(&self) -> Option<(Self::JobId, Self::Job)> {
        let mut storage = self.prover_connection_pool.access_storage().await;
        let prover_job = match self.circuit_ids_for_round_to_be_proven.is_empty() {
            false => {
                // Specialized prover: proving subset of configured circuits.
                storage
                    .fri_prover_jobs_dal()
                    .get_next_job_for_circuit_id_round(&self.circuit_ids_for_round_to_be_proven)
                    .await
            }
            true => {
                // Generalized prover: proving all circuits.
                storage.fri_prover_jobs_dal().get_next_job().await
            }
        }?;
        vlog::info!("Started processing prover job: {:?}", prover_job);

        let circuit_key = FriCircuitKey {
            block_number: prover_job.block_number,
            sequence_number: prover_job.sequence_number,
            circuit_id: prover_job.circuit_id,
            aggregation_round: prover_job.aggregation_round,
            depth: prover_job.depth,
        };
        let started_at = Instant::now();
        let input = self
            .blob_store
            .get(circuit_key)
            .await
            .unwrap_or_else(|err| panic!("{err:?}"));
        metrics::histogram!(
                "prover_fri.prover.blob_fetch_time",
                started_at.elapsed(),
                "circuit_type" => prover_job.circuit_id.to_string(),
                "aggregation_round" => format!("{:?}", prover_job.aggregation_round),
        );
        let setup_data_key = ProverServiceDataKey {
            circuit_id: prover_job.circuit_id,
            round: prover_job.aggregation_round,
        };

        Some((
            prover_job.id,
            ProverJob::new(
                prover_job.block_number,
                prover_job.id,
                input,
                setup_data_key,
            ),
        ))
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
        let setup_data = self.get_setup_data(job.setup_data_key.clone());
        tokio::task::spawn_blocking(move || Self::prove(job, config, setup_data))
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) {
        vlog::info!(
            "Successfully proven job: {}, took: {:?}",
            job_id,
            started_at.elapsed()
        );
        let proof = artifacts.proof_wrapper;

        // We save the scheduler proofs in public bucket,
        // so that it can be verified independently while we're doing shadow proving
        let circuit_type = match &proof {
            FriProofWrapper::Base(base) => base.numeric_circuit_type(),
            FriProofWrapper::Recursive(recursive_circuit) => match recursive_circuit {
                ZkSyncRecursionLayerProof::SchedulerCircuit(_) => {
                    self.public_blob_store
                        .put(artifacts.block_number.0, &proof)
                        .await
                        .unwrap();
                    recursive_circuit.numeric_circuit_type()
                }
                _ => recursive_circuit.numeric_circuit_type(),
            },
        };

        let blob_save_started_at = Instant::now();
        let blob_url = self.blob_store.put(job_id, &proof).await.unwrap();
        metrics::histogram!(
                "prover_fri.prover.blob_save_time",
                blob_save_started_at.elapsed(),
                "circuit_type" => circuit_type.to_string(),
        );

        let mut prover_connection = self.prover_connection_pool.access_storage().await;
        let mut transaction = prover_connection.start_transaction().await;
        let job_metadata = transaction
            .fri_prover_jobs_dal()
            .save_proof(job_id, started_at.elapsed(), &blob_url)
            .await;
        if job_metadata.is_node_final_proof {
            transaction
                .fri_scheduler_dependency_tracker_dal()
                .set_final_prover_job_id_for_l1_batch(
                    get_base_layer_circuit_id_for_recursive_layer(job_metadata.circuit_id),
                    job_id,
                    job_metadata.block_number,
                )
                .await;
        }
        transaction.commit().await;
    }
}
