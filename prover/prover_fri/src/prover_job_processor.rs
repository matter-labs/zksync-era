use std::collections::HashMap;
use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::task::JoinHandle;
use zksync_prover_fri_types::circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::pow::NoPow;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::{
    ZkSyncBaseLayerCircuit, ZkSyncBaseLayerProof,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerProof, ZkSyncRecursiveLayerCircuit,
};
use zksync_prover_fri_types::circuit_definitions::{
    base_layer_proof_config, recursion_layer_proof_config, ZkSyncDefaultRoundFunction,
};

use zkevm_test_harness::prover_utils::{prove_base_layer_circuit, prove_recursion_layer_circuit};

use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_config::configs::FriProverConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::{CircuitWrapper, FriProofWrapper, ProverJob, ProverServiceDataKey};
use zksync_prover_fri_utils::fetch_next_circuit;
use zksync_queued_job_processor::{async_trait, JobProcessor};
use zksync_types::{basic_fri_types::CircuitIdRoundTuple, protocol_version::L1VerifierConfig};
use zksync_vk_setup_data_server_fri::{
    get_cpu_setup_data_for_circuit_type, GoldilocksProverSetupData,
};

use crate::utils::{
    get_setup_data_key, save_proof, setup_metadata_to_setup_data_key, verify_proof, ProverArtifacts,
};

pub enum SetupLoadMode {
    FromMemory(HashMap<ProverServiceDataKey, Arc<GoldilocksProverSetupData>>),
    FromDisk,
}

pub struct Prover {
    blob_store: Box<dyn ObjectStore>,
    public_blob_store: Option<Box<dyn ObjectStore>>,
    config: Arc<FriProverConfig>,
    prover_connection_pool: ConnectionPool,
    setup_load_mode: SetupLoadMode,
    // Only pick jobs for the configured circuit id and aggregation rounds.
    // Empty means all jobs are picked.
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
    vk_commitments: L1VerifierConfig,
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
        vk_commitments: L1VerifierConfig,
    ) -> Self {
        Prover {
            blob_store,
            public_blob_store,
            config: Arc::new(config),
            prover_connection_pool,
            setup_load_mode,
            circuit_ids_for_round_to_be_proven,
            vk_commitments,
        }
    }

    fn get_setup_data(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<Arc<GoldilocksProverSetupData>> {
        let key = get_setup_data_key(key);
        Ok(match &self.setup_load_mode {
            SetupLoadMode::FromMemory(cache) => cache
                .get(&key)
                .context("Setup data not found in cache")?
                .clone(),
            SetupLoadMode::FromDisk => {
                let started_at = Instant::now();
                let artifact: GoldilocksProverSetupData =
                    get_cpu_setup_data_for_circuit_type(key.clone())
                        .context("get_cpu_setup_data_for_circuit_type()")?;
                metrics::histogram!(
                    "prover_fri.prover.setup_data_load_time",
                    started_at.elapsed(),
                    "circuit_type" => key.circuit_id.to_string(),
                );
                Arc::new(artifact)
            }
        })
    }

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
        _config: Arc<FriProverConfig>,
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
        verify_proof(
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
        _config: Arc<FriProverConfig>,
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
        verify_proof(&CircuitWrapper::Base(circuit), &proof, &artifact.vk, job_id);
        FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
    }
}

#[async_trait]
impl JobProcessor for Prover {
    type Job = ProverJob;
    type JobId = u32;
    type JobArtifacts = ProverArtifacts;
    const SERVICE_NAME: &'static str = "FriCpuProver";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut storage = self.prover_connection_pool.access_storage().await.unwrap();
        let Some(prover_job) = fetch_next_circuit(
            &mut storage,
            &*self.blob_store,
            &self.circuit_ids_for_round_to_be_proven,
            &self.vk_commitments,
        )
        .await
        else {
            return Ok(None);
        };
        Ok(Some((prover_job.job_id, prover_job)))
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
        let config = Arc::clone(&self.config);
        let setup_data = self.get_setup_data(job.setup_data_key.clone());
        tokio::task::spawn_blocking(move || {
            Ok(Self::prove(
                job,
                config,
                setup_data.context("get_setup_data()")?,
            ))
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
    ) -> anyhow::Result<()> {
        metrics::histogram!(
            "prover_fri.prover.cpu_total_proving_time",
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

#[allow(dead_code)]
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
                .expect(
                    "At least one circuit should be configured for group when running in FromMemory mode",
                );
            tracing::info!(
                "for group {} configured setup metadata are {:?}",
                &config.specialized_group_id,
                prover_setup_metadata_list
            );
            for prover_setup_metadata in prover_setup_metadata_list {
                let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
                let setup_data = get_cpu_setup_data_for_circuit_type(key.clone())
                    .context("get_cpu_setup_data_for_circuit_type()")?;
                cache.insert(key, Arc::new(setup_data));
            }
            SetupLoadMode::FromMemory(cache)
        }
    })
}
