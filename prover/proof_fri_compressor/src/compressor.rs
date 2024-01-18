use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::task::JoinHandle;
use zkevm_test_harness::proof_wrapper_utils::{wrap_proof, WrapperConfig};
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
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
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    aggregated_operations::L1BatchProofForL1,
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::{
            ZkSyncCircuit, ZkSyncProof, ZkSyncVerificationKey,
        },
        bellman::{bn256::Bn256, plonk::better_better_cs::proof::Proof},
        witness::oracle::VmWitnessOracle,
    },
    L1BatchNumber,
};
use zksync_vk_setup_data_server_fri::{get_recursive_layer_vk_for_circuit_type, get_snark_vk};

use crate::metrics::METRICS;

pub struct ProofCompressor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool,
    compression_mode: u8,
    verify_wrapper_proof: bool,
    max_attempts: u32,
}

impl ProofCompressor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool,
        compression_mode: u8,
        verify_wrapper_proof: bool,
        max_attempts: u32,
    ) -> Self {
        Self {
            blob_store,
            pool,
            compression_mode,
            verify_wrapper_proof,
            max_attempts,
        }
    }

    pub fn compress_proof(
        proof: ZkSyncRecursionLayerProof,
        compression_mode: u8,
        verify_wrapper_proof: bool,
    ) -> anyhow::Result<Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>> {
        let scheduler_vk = get_recursive_layer_vk_for_circuit_type(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        )
        .context("get_recursiver_layer_vk_for_circuit_type()")?;
        let config = WrapperConfig::new(compression_mode);

        let (wrapper_proof, _) = wrap_proof(proof, scheduler_vk, config);
        let inner = wrapper_proof.into_inner();
        // (Re)serialization should always succeed.
        // TODO: is that true here?
        let serialized = bincode::serialize(&inner)
            .expect("Failed to serialize proof with ZkSyncSnarkWrapperCircuit");
        let proof: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>> =
            bincode::deserialize(&serialized)
                .expect("Failed to deserialize proof with ZkSyncCircuit");
        if verify_wrapper_proof {
            let existing_vk = get_snark_vk().context("get_snark_vk()")?;
            let vk = ZkSyncVerificationKey::from_verification_key_and_numeric_type(0, existing_vk);
            let scheduler_proof = ZkSyncProof::from_proof_and_numeric_type(0, proof.clone());
            match vk.verify_proof(&scheduler_proof) {
                true => tracing::info!("Compressed proof verified successfully"),
                false => anyhow::bail!("Compressed proof verification failed "),
            }
        }
        Ok(proof)
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
    type JobArtifacts = Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>;
    const SERVICE_NAME: &'static str = "ProofCompressor";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut conn = self.pool.access_storage().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(l1_batch_number) = conn
            .fri_proof_compressor_dal()
            .get_next_proof_compression_job(&pod_name)
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
            .access_storage()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_failed(&error, job_id)
            .await;
    }

    async fn process_job(
        &self,
        job: ZkSyncRecursionLayerProof,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let compression_mode = self.compression_mode;
        let verify_wrapper_proof = self.verify_wrapper_proof;
        tokio::task::spawn_blocking(move || {
            Self::compress_proof(job, compression_mode, verify_wrapper_proof)
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
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
        };
        let blob_save_started_at = Instant::now();
        let blob_url = self
            .blob_store
            .put(job_id, &l1_batch_proof)
            .await
            .context("Failed to save converted l1_batch_proof")?;
        METRICS
            .blob_save_time
            .observe(blob_save_started_at.elapsed());

        self.pool
            .access_storage()
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
            .access_storage()
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
