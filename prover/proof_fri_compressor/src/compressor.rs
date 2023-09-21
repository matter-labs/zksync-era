use async_trait::async_trait;
use std::time::Instant;
use tokio::task::JoinHandle;

use zkevm_test_harness::proof_wrapper_utils::wrap_proof;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
};
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness;
use zksync_prover_fri_types::{AuxOutputWitnessWrapper, FriProofWrapper, get_current_pod_name};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::aggregated_operations::L1BatchProofForL1;
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::bellman::plonk::better_better_cs::proof::Proof;
use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_types::L1BatchNumber;
use zksync_vk_setup_data_server_fri::get_recursive_layer_vk_for_circuit_type;

pub struct ProofCompressor {
    blob_store: Box<dyn ObjectStore>,
    pool: ConnectionPool,
    compression_mode: u8,
}

impl ProofCompressor {
    pub fn new(
        blob_store: Box<dyn ObjectStore>,
        pool: ConnectionPool,
        compression_mode: u8,
    ) -> Self {
        Self {
            blob_store,
            pool,
            compression_mode,
        }
    }

    pub fn compress_proof(
        proof: ZkSyncRecursionLayerProof,
        compression_mode: u8,
    ) -> Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>> {
        let scheduler_vk = get_recursive_layer_vk_for_circuit_type(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        );
        let (wrapper_proof, _) = wrap_proof(proof, scheduler_vk, compression_mode);
        let inner = wrapper_proof.into_inner();
        let serialized = bincode::serialize(&inner)
            .expect("Failed to serialize proof with ZkSyncSnarkWrapperCircuit");
        bincode::deserialize(&serialized).expect("Failed to deserialize proof with ZkSyncCircuit")
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

    async fn get_next_job(&self) -> Option<(Self::JobId, Self::Job)> {
        let mut conn = self.pool.access_storage().await;
        let pod_name = get_current_pod_name();
        let l1_batch_number = conn
            .fri_proof_compressor_dal()
            .get_next_proof_compression_job(&pod_name)
            .await?;
        let fri_proof_id = conn
            .fri_prover_jobs_dal()
            .get_scheduler_proof_job_id(l1_batch_number)
            .await?;
        tracing::info!(
            "Started proof compression for L1 batch: {:?}",
            l1_batch_number
        );
        let started_at = Instant::now();
        let fri_proof: FriProofWrapper = self.blob_store.get(fri_proof_id)
            .await
            .unwrap_or_else(|_| panic!("Failed to get fri proof from blob store for {l1_batch_number} with id {fri_proof_id}"));
        metrics::histogram!(
            "prover_fri.proof_fri_compressor.blob_fetch_time",
            started_at.elapsed(),
        );
        let scheduler_proof = match fri_proof {
            FriProofWrapper::Base(_) => {
                panic!("Must be a scheduler proof not base layer")
            }
            FriProofWrapper::Recursive(proof) => proof,
        };
        Some((l1_batch_number, scheduler_proof))
    }

    async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
        self.pool
            .access_storage()
            .await
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_failed(&error, job_id)
            .await;
    }

    async fn process_job(
        &self,
        job: ZkSyncRecursionLayerProof,
        _started_at: Instant,
    ) -> JoinHandle<Self::JobArtifacts> {
        let compression_mode = self.compression_mode;
        tokio::task::spawn_blocking(move || Self::compress_proof(job, compression_mode))
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    ) {
        metrics::histogram!(
            "prover_fri.proof_fri_compressor.compression_time",
            started_at.elapsed(),
        );
        tracing::info!(
            "Finished fri proof compression for job: {job_id} took: {:?}",
            started_at.elapsed()
        );

        let aux_output_witness_wrapper: AuxOutputWitnessWrapper = self
            .blob_store
            .get(job_id)
            .await
            .expect("Failed to get aggregation result coords from blob store");
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
            .expect("Failed to save converted l1_batch_proof");
        metrics::histogram!(
            "prover_fri.proof_fri_compressor.blob_save_time",
            blob_save_started_at.elapsed(),
        );
        self.pool
            .access_storage()
            .await
            .fri_proof_compressor_dal()
            .mark_proof_compression_job_successful(job_id, started_at.elapsed(), &blob_url)
            .await;
    }
}
