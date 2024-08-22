use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use fflonk::{bellman::worker::Worker, CompressionInput, CompressionMode, CompressionSchedule, FflonkSnarkVerifierCircuit, FflonkSnarkVerifierCircuitProof};
use tokio::task::JoinHandle;
#[allow(unused_imports)]
use zkevm_test_harness::proof_wrapper_utils::{get_trusted_setup, wrap_proof};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::aux_layer::{
    wrapper::ZkSyncCompressionWrapper, ZkSyncCompressionProofForWrapper,
    ZkSyncCompressionVerificationKeyForWrapper,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionProof, ZkSyncRecursionVerificationKey,
};
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

    pub fn fflonk_compress_proof(
        proof: ZkSyncRecursionProof,
        vk: ZkSyncRecursionVerificationKey,
        schedule: CompressionSchedule,
    ) -> (
        ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    ) {
        let worker = franklin_crypto::boojum::worker::Worker::new();
        let mut input = CompressionInput::RecursionLayer(Some(proof), vk, CompressionMode::One);

        dbg!(&schedule);
        let CompressionSchedule {
            compression_steps, ..
        } = schedule;

        let last_compression_wrapping_mode = CompressionMode::from_compression_mode(
            compression_steps.last().unwrap().clone() as u8 + 1,
        );
        dbg!(&last_compression_wrapping_mode);

        let num_compression_steps = compression_steps.len();
        let mut compression_modes_iter = compression_steps.into_iter();
        for step_idx in 0..num_compression_steps {
            let compression_mode = compression_modes_iter.next().unwrap();
            let compression_circuit = input.into_compression_circuit();
            println!("Proving compression {}", compression_mode as u8);
            let (proof, vk) =
                fflonk::inner_prove_compression_layer_circuit(compression_circuit, &worker);
            println!(
                "Proof for compression {} is generated!",
                compression_mode as u8
            );

            if step_idx + 1 == num_compression_steps {
                input = CompressionInput::CompressionWrapperLayer(
                    Some(proof),
                    vk,
                    last_compression_wrapping_mode,
                );
            } else {
                input = CompressionInput::CompressionLayer(
                    Some(proof),
                    vk,
                    CompressionMode::from_compression_mode(compression_mode as u8 + 1),
                );
            }
        }

        // last wrapping step
        println!(
            "Proving compression {} for wrapper",
            last_compression_wrapping_mode as u8
        );
        let compression_circuit = input.into_compression_wrapper_circuit();
        let (proof, vk) =
            fflonk::inner_prove_compression_wrapper_circuit(compression_circuit, &worker);
        println!(
            "Proof for compression wrapper {} is generated!",
            last_compression_wrapping_mode as u8
        );
        (proof, vk)
    }

    #[tracing::instrument(skip(proof, _compression_mode))]
    pub fn compress_proof(
        l1_batch: L1BatchNumber,
        proof: ZkSyncRecursionLayerProof,
        _compression_mode: u8,
    ) -> anyhow::Result<FflonkSnarkVerifierCircuitProof> {
        println!("start comrpession");
        let keystore = Keystore::default();
        let scheduler_vk = keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .context("get_recursiver_layer_vk_for_circuit_type()")?;

        // set compression schedule:
        // - "hard" is the strategy that gives smallest final circuit
        let compression_schedule = CompressionSchedule::hard();
        let compression_wrapper_mode = compression_schedule
            .compression_steps
            .last()
            .unwrap()
            .clone() as u8
            + 1;
        // compress proof step by step: 1 -> 2 -> 3 -> 4 -> 5(wrapper)
        let (compression_wrapper_proof, compression_wrapper_vk) = Self::fflonk_compress_proof(
            proof.into_inner(),
            scheduler_vk.into_inner(),
            compression_schedule,
        );

        // construct fflonk snark verifier circuit
        let wrapper_function =
            ZkSyncCompressionWrapper::from_numeric_circuit_type(compression_wrapper_mode);
        let fixed_parameters = compression_wrapper_vk.fixed_parameters.clone();
        let circuit = FflonkSnarkVerifierCircuit {
            witness: Some(compression_wrapper_proof),
            vk: compression_wrapper_vk,
            fixed_parameters,
            transcript_params: (),
            wrapper_function,
        };
        // create fflonk proof in single shot - without precomputation
        let (proof, _) =
            fflonk::prove_fflonk_snark_verifier_circuit_single_shot(&circuit, &Worker::new());
        println!("finished proof");
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
    type JobArtifacts = FflonkSnarkVerifierCircuitProof;
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
            Self::compress_proof(block_number, job, compression_mode)
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: FflonkSnarkVerifierCircuitProof,
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
