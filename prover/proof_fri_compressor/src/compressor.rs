use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_sequencer_api::proof::FinalProof;
use tokio::task::JoinHandle;
#[cfg(feature = "gpu")]
use wrapper_prover::{Bn256, GPUWrapperConfigs, WrapperProver, DEFAULT_WRAPPER_CONFIG};
#[cfg(not(feature = "gpu"))]
use zkevm_test_harness::proof_wrapper_utils::WrapperConfig;
#[allow(unused_imports)]
use zkevm_test_harness::proof_wrapper_utils::{get_trusted_setup, wrap_proof};
use zkevm_test_harness::proof_wrapper_utils::{wrap_proof, WrapperConfig};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::recursion_layer::{
            ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
        },
        zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
    },
    get_current_pod_name, AuxOutputWitnessWrapper, FriProofWrapper,
};
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber};
use zksync_vk_setup_data_server_fri::keystore::Keystore;

use crate::metrics::METRICS;

pub struct ProofCompressor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
    compression_mode: u8,
    max_attempts: u32,
    protocol_version: ProtocolSemanticVersion,
}

impl ProofCompressor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
        compression_mode: u8,
        max_attempts: u32,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            blob_store,
            pool,
            compression_mode,
            max_attempts,
            protocol_version,
        }
    }

    fn verify_proof(keystore: Keystore, serialized_proof: Vec<u8>) -> anyhow::Result<()> {
        let proof: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>> =
            bincode::deserialize(&serialized_proof)
                .expect("Failed to deserialize proof with ZkSyncCircuit");
        // We're fetching the key as String and deserializing it here
        // as we don't want to include the old version of prover in the main libraries.
        let existing_vk_serialized = keystore
            .load_snark_verification_key()
            .context("get_snark_vk()")?;
        let existing_vk = serde_json::from_str::<
            SnarkVerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
        >(&existing_vk_serialized)?;

        let vk = ZkSyncVerificationKey::from_verification_key_and_numeric_type(0, existing_vk);
        let scheduler_proof = ZkSyncProof::from_proof_and_numeric_type(0, proof.clone());
        match vk.verify_proof(&scheduler_proof) {
            true => tracing::info!("Compressed proof verified successfully"),
            false => anyhow::bail!("Compressed proof verification failed "),
        }
        Ok(())
    }
    pub fn compress_proof(
        proof: ZkSyncRecursionLayerProof,
        _compression_mode: u8,
    ) -> anyhow::Result<FinalProof> {
        let keystore = Keystore::default();
        let scheduler_vk = keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .context("get_recursiver_layer_vk_for_circuit_type()")?;

        #[cfg(feature = "gpu")]
        let wrapper_proof = {
            let crs = get_trusted_setup();
            let wrapper_config = DEFAULT_WRAPPER_CONFIG;
            let mut prover = WrapperProver::<GPUWrapperConfigs>::new(&crs, wrapper_config).unwrap();

            prover
                .generate_setup_data(scheduler_vk.into_inner())
                .unwrap();
            prover.generate_proofs(proof.into_inner()).unwrap();

            prover.get_wrapper_proof().unwrap()
        };
        #[cfg(not(feature = "gpu"))]
        let wrapper_proof = {
            let config = WrapperConfig::new(_compression_mode);

            let (wrapper_proof, _) = wrap_proof(proof, scheduler_vk, config);
            wrapper_proof.into_inner()
        };

        // (Re)serialization should always succeed.
        let serialized = bincode::serialize(&wrapper_proof)
            .expect("Failed to serialize proof with ZkSyncSnarkWrapperCircuit");

        // For sending to L1, we can use the `FinalProof` type, that has a generic circuit inside, that is not used for serialization.
        // So `FinalProof` and `Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>` are compatible on serialization bytecode level.
        let final_proof: FinalProof =
            bincode::deserialize(&serialized).expect("Failed to deserialize final proof");
        Ok(final_proof)
    }

    fn aux_output_witness_to_array(
        aux_output_witness: BlockAuxilaryOutputWitness<GoldilocksField>,
    ) -> [[u8; 32]; 4] {
        let mut array: [[u8; 32]; 4] = [[0; 32]; 4];

        for i in 0..32 {
            array[0][i] = aux_output_witness.l1_messages_linear_hash[i];
            array[1][i] = aux_output_witness.rollup_state_diff_for_compression[i];
            array[2][i] = aux_output_witness.bootloader_heap_initial_content[i];
            array[3][i] = aux_output_witness.events_queue_state[i];
        }
        array
    }
}

#[async_trait]
impl JobProcessor for ProofCompressor {
    type Job = ZkSyncRecursionLayerProof;
    type JobId = L1BatchNumber;
    type JobArtifacts = FinalProof;
    const SERVICE_NAME: &'static str = "ProofCompressor";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut conn = self.pool.connection().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(l1_batch_number) = conn
            .fri_proof_compressor_dal()
            .get_next_proof_compression_job(&pod_name, self.protocol_version)
            .await
        else {
            return Ok(None);
        };
        let Some(fri_proof_id) = conn
            .fri_prover_jobs_dal()
            .get_scheduler_proof_job_id(l1_batch_number)
            .await
        else {
            return Ok(None);
        };
        tracing::info!(
            "Started proof compression for L1 batch: {:?}",
            l1_batch_number
        );
        let observer = METRICS.blob_fetch_time.start();

        let fri_proof: FriProofWrapper = self.blob_store.get(fri_proof_id)
            .await.with_context(|| format!("Failed to get fri proof from blob store for {l1_batch_number} with id {fri_proof_id}"))?;

        observer.observe();

        let scheduler_proof = match fri_proof {
            FriProofWrapper::Base(_) => anyhow::bail!("Must be a scheduler proof not base layer"),
            FriProofWrapper::Recursive(proof) => proof,
        };
        Ok(Some((l1_batch_number, scheduler_proof)))
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_failed(&error, job_id)
            .await;
    }

    async fn process_job(
        &self,
        job_id: &L1BatchNumber,
        job: ZkSyncRecursionLayerProof,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let compression_mode = self.compression_mode;
        let block_number = *job_id;
        tokio::task::spawn_blocking(move || {
            let _span = tracing::info_span!("compress", %block_number).entered();
            Self::compress_proof(job, compression_mode)
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: FinalProof,
    ) -> anyhow::Result<()> {
        METRICS.compression_time.observe(started_at.elapsed());
        tracing::info!(
            "Finished fri proof compression for job: {job_id} took: {:?}",
            started_at.elapsed()
        );

        let aux_output_witness_wrapper: AuxOutputWitnessWrapper = self
            .blob_store
            .get(job_id)
            .await
            .context("Failed to get aggregation result coords from blob store")?;
        let aggregation_result_coords =
            Self::aux_output_witness_to_array(aux_output_witness_wrapper.0);
        let l1_batch_proof = L1BatchProofForL1 {
            aggregation_result_coords,
            scheduler_proof: artifacts,
            protocol_version: self.protocol_version,
        };
        let blob_save_started_at = Instant::now();
        let blob_url = self
            .blob_store
            .put((job_id, self.protocol_version), &l1_batch_proof)
            .await
            .context("Failed to save converted l1_batch_proof")?;
        METRICS
            .blob_save_time
            .observe(blob_save_started_at.elapsed());

        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_successful(job_id, started_at.elapsed(), &blob_url)
            .await;
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .pool
            .connection()
            .await
            .context("failed to acquire DB connection for ProofCompressor")?;
        prover_storage
            .fri_proof_compressor_dal()
            .get_proof_compression_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for ProofCompressor")
    }
}
