use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_sequencer_api::proof::FinalProof;
use fflonk_gpu::{FflonkSnarkVerifierCircuit, FflonkSnarkVerifierCircuitProof};
use tokio::task::JoinHandle;
use wrapper_prover::{GPUWrapperConfigs, WrapperProver};
use zkevm_test_harness::proof_wrapper_utils::{get_trusted_setup, DEFAULT_WRAPPER_CONFIG};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::{
            aux_layer::{
                wrapper::ZkSyncCompressionWrapper, ZkSyncCompressionForWrapperCircuit,
                ZkSyncCompressionLayerCircuit, ZkSyncCompressionProof,
                ZkSyncCompressionProofForWrapper, ZkSyncCompressionVerificationKeyForWrapper,
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
use zksync_prover_interface::outputs::{
    FflonkL1BatchProofForL1, L1BatchProofForL1, PlonkL1BatchProofForL1,
};
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
    is_fflonk: bool,
}

pub enum Proof {
    Plonk(Box<FinalProof>),
    Fflonk(FflonkSnarkVerifierCircuitProof),
}

impl ProofCompressor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
        compression_mode: u8,
        max_attempts: u32,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
        is_fflonk: bool,
    ) -> Self {
        Self {
            blob_store,
            pool,
            compression_mode,
            max_attempts,
            protocol_version,
            keystore,
            is_fflonk,
        }
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

    #[tracing::instrument(skip(proof, _compression_mode))]
    pub fn generate_plonk_proof(
        proof: ZkSyncRecursionLayerProof,
        _compression_mode: u8,
        keystore: Keystore,
    ) -> anyhow::Result<FinalProof> {
        let scheduler_vk = keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .context("get_recursiver_layer_vk_for_circuit_type()")?;

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

        // (Re)serialization should always succeed.
        let serialized = bincode::serialize(&wrapper_proof)
            .expect("Failed to serialize proof with ZkSyncSnarkWrapperCircuit");

        // For sending to L1, we can use the `FinalProof` type, that has a generic circuit inside, that is not used for serialization.
        // So `FinalProof` and `Proof<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>` are compatible on serialization bytecode level.
        let final_proof: FinalProof =
            bincode::deserialize(&serialized).expect("Failed to deserialize final proof");
        Ok(final_proof)
    }

    #[tracing::instrument(skip(proof, compression_mode, keystore))]
    pub fn generate_fflonk_proof(
        proof: ZkSyncRecursionLayerProof,
        compression_mode: u8,
        keystore: Keystore,
    ) -> anyhow::Result<FflonkSnarkVerifierCircuitProof> {
        let scheduler_vk = keystore
            .load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .context("get_recursiver_layer_vk_for_circuit_type()")?;

        // compress proof step by step: 1 -> 2 -> 3 -> 4 -> 5(wrapper)
        let (compression_wrapper_proof, compression_wrapper_vk) = Self::compress_proof(
            &keystore,
            proof.into_inner(),
            scheduler_vk.into_inner(),
            compression_mode,
        )?;

        // construct fflonk snark verifier circuit
        let wrapper_function =
            ZkSyncCompressionWrapper::from_numeric_circuit_type(compression_mode);
        let fixed_parameters = compression_wrapper_vk.fixed_parameters.clone();
        let circuit = FflonkSnarkVerifierCircuit {
            witness: Some(compression_wrapper_proof),
            vk: compression_wrapper_vk,
            fixed_parameters,
            transcript_params: (),
            wrapper_function,
        };

        tracing::info!("Proving FFLONK snark verifier");

        let setup = keystore.load_fflonk_snark_verifier_setup_data()?;

        tracing::info!("Loaded setup data for FFLONK verification");

        let proof = fflonk_gpu::gpu_prove_fflonk_snark_verifier_circuit_with_precomputation(
            &circuit,
            &setup,
            &setup.get_verification_key(),
        );
        tracing::info!("Finished proof generation");
        Ok(proof)
    }

    pub fn compress_proof(
        keystore: &Keystore,
        proof: ZkSyncCompressionProof,
        vk: ZkSyncRecursionVerificationKey,
        compression_steps: u8,
    ) -> anyhow::Result<(
        ZkSyncCompressionProofForWrapper,
        ZkSyncCompressionVerificationKeyForWrapper,
    )> {
        let worker = franklin_crypto::boojum::worker::Worker::new();
        let mut compression_circuit =
            ZkSyncCompressionLayerCircuit::from_witness_and_vk(Some(proof), vk.clone(), 1);
        let mut compression_wrapper_circuit = None;

        for step_idx in 1..compression_steps {
            tracing::info!("Proving compression {:?}", step_idx);
            let setup_data = keystore.load_compression_setup_data(step_idx)?;
            let (proof, vk) =
                proof_compression_gpu::prove_compression_layer_circuit_with_precomputations(
                    compression_circuit.clone(),
                    &setup_data.setup,
                    setup_data.finalization_hint,
                    setup_data.vk,
                    &worker,
                );
            tracing::info!("Proof for compression {:?} is generated!", step_idx);

            if step_idx + 1 == compression_steps {
                compression_wrapper_circuit =
                    Some(ZkSyncCompressionForWrapperCircuit::from_witness_and_vk(
                        Some(proof),
                        vk,
                        compression_steps,
                    ));
            } else {
                compression_circuit = ZkSyncCompressionLayerCircuit::from_witness_and_vk(
                    Some(proof),
                    vk,
                    step_idx + 1,
                );
            }
        }

        // last wrapping step
        tracing::info!("Proving compression {} for wrapper", compression_steps);

        let setup_data = keystore.load_compression_wrapper_setup_data(compression_steps)?;
        let (proof, vk) =
            proof_compression_gpu::prove_compression_wrapper_circuit_with_precomputations(
                compression_wrapper_circuit.unwrap(),
                &setup_data.setup,
                setup_data.finalization_hint,
                setup_data.vk,
                &worker,
            );
        tracing::info!(
            "Proof for compression wrapper {} is generated!",
            compression_steps
        );
        Ok((proof, vk))
    }
}

#[async_trait]
impl JobProcessor for ProofCompressor {
    type Job = ZkSyncRecursionLayerProof;
    type JobId = L1BatchNumber;

    type JobArtifacts = Proof;

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
        _job_id: &L1BatchNumber,
        job: ZkSyncRecursionLayerProof,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
        let compression_mode = self.compression_mode;
        let keystore = self.keystore.clone();
        let is_fflonk = self.is_fflonk;
        tokio::task::spawn_blocking(move || {
            if !is_fflonk {
                Ok(Proof::Plonk(Box::new(Self::generate_plonk_proof(
                    job,
                    compression_mode,
                    keystore,
                )?)))
            } else {
                Ok(Proof::Fflonk(Self::generate_fflonk_proof(
                    job,
                    compression_mode,
                    keystore,
                )?))
            }
        })
    }

    async fn save_result(
        &self,
        job_id: Self::JobId,
        started_at: Instant,
        artifacts: Self::JobArtifacts,
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

        let l1_batch_proof = match artifacts {
            Proof::Plonk(proof) => L1BatchProofForL1::Plonk(PlonkL1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof: *proof,
                protocol_version: self.protocol_version,
            }),
            Proof::Fflonk(proof) => L1BatchProofForL1::Fflonk(FflonkL1BatchProofForL1 {
                aggregation_result_coords,
                scheduler_proof: proof,
                protocol_version: self.protocol_version,
            }),
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
