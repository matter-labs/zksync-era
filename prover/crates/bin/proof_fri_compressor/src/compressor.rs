use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use fflonk_gpu::{
    bellman::worker::Worker,
    fflonk::circuit_definitions::circuit_definitions::aux_layer::ZkSyncCompressionProof,
    FflonkSnarkVerifierCircuit, FflonkSnarkVerifierCircuitProof,
};
use proof_compression_gpu::{CompressionInput, CompressionMode, CompressionSchedule};
use tokio::task::JoinHandle;
use tracing::Instrument;
#[allow(unused_imports)]
use zkevm_test_harness::proof_wrapper_utils::{get_trusted_setup, wrap_proof};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::{
            aux_layer::{
                wrapper::ZkSyncCompressionWrapper, ZkSyncCompressionProofForWrapper,
                ZkSyncCompressionVerificationKeyForWrapper,
            },
            recursion_layer::{
                ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
                ZkSyncRecursionVerificationKey,
            },
        },
        zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness,
    },
    get_current_pod_name, AuxOutputWitnessWrapper, FriProofWrapper,
};
use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber};

use crate::metrics::METRICS;

pub struct ProofCompressor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
    compression_mode: u8,
    max_attempts: u32,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
}

impl ProofCompressor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
        compression_mode: u8,
        max_attempts: u32,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
    ) -> Self {
        Self {
            blob_store,
            pool,
            compression_mode,
            max_attempts,
            protocol_version,
            keystore,
        }
    }

    pub fn fflonk_compress_proof(
        keystore: Keystore,
        proof: ZkSyncCompressionProof,
        vk: ZkSyncRecursionVerificationKey,
        schedule: CompressionSchedule,
    ) -> anyhow::Result<(
        ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    )> {
        let setup_data = keystore.load_compression_wrapper_setup_data()?;

        let worker = franklin_crypto::boojum::worker::Worker::new();
        let mut input = CompressionInput::Compression(Some(proof), vk, CompressionMode::One);

        tracing::debug!("Compression schedule: {:?}", &schedule);
        let CompressionSchedule {
            compression_steps, ..
        } = schedule;

        let last_compression_wrapping_mode = CompressionMode::from_compression_mode(
            compression_steps.last().unwrap().clone() as u8 + 1,
        );
        tracing::debug!(
            "Compression wrapping mode: {:?}",
            &last_compression_wrapping_mode
        );

        for (step_idx, compression_mode) in compression_steps.clone().iter_mut().enumerate() {
            let compression_circuit = input.into_compression_circuit();
            tracing::info!("Proving compression {:?}", compression_mode);
            let (proof, vk) = proof_compression_gpu::prove_compression_layer_circuit(
                compression_circuit,
                &mut None,
                &worker,
            );
            tracing::info!("Proof for compression {:?} is generated!", compression_mode);

            if step_idx + 1 == compression_steps.len() {
                std::fs::write("compression_proof.bin", bincode::serialize(&proof).unwrap()) // todo: I believe this should be removed
                    .unwrap();
                std::fs::write("compression_vk.json", serde_json::to_string(&vk).unwrap()).unwrap(); // todo: this should be removed as well
                input = CompressionInput::CompressionWrapper(
                    Some(proof),
                    vk,
                    last_compression_wrapping_mode,
                );
            } else {
                input = CompressionInput::Compression(
                    Some(proof),
                    vk,
                    CompressionMode::from_compression_mode(*compression_mode as u8 + 1),
                );
            }
        }

        // last wrapping step
        tracing::info!(
            "Proving compression {} for wrapper",
            last_compression_wrapping_mode as u8
        );
        let compression_circuit = input.into_compression_wrapper_circuit();
        let (proof, vk) = proof_compression_gpu::prove_compression_wrapper_circuit(
            compression_circuit,
            &mut Some(setup_data),
            &worker,
        );
        tracing::info!(
            "Proof for compression wrapper {} is generated!",
            last_compression_wrapping_mode as u8
        );
        Ok((proof, vk))
    }

    #[tracing::instrument(skip(proof, _compression_mode))]
    pub fn compress_proof(
        proof: ZkSyncRecursionLayerProof,
        _compression_mode: u8,
        keystore: Keystore,
    ) -> anyhow::Result<FflonkSnarkVerifierCircuitProof> {
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
            keystore,
            proof.into_inner(),
            scheduler_vk.into_inner(),
            compression_schedule,
        )?;

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
        let (proof, _) = fflonk_gpu::gpu_prove_fflonk_snark_verifier_circuit_single_shot(
            &circuit,
            &Worker::new(),
        );
        tracing::info!("Finished proof generation");
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
            anyhow::bail!("Scheduler proof is missing from database for batch {l1_batch_number}");
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
        let keystore = self.keystore.clone();
        tokio::task::spawn_blocking(move || {
            Self::compress_proof(job, compression_mode, keystore)
                .instrument(tracing::info_span!("Compress_proof", batch_number = %job_id))
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
