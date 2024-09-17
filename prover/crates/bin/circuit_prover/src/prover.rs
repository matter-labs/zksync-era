use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context};
use shivini::{
    gpu_proof_config::GpuProofConfig, gpu_prove_from_external_witness_data, ProverContext,
    ProverContextConfig,
};
use tokio::{sync::mpsc::Receiver, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use zkevm_test_harness::prover_utils::{verify_base_layer_proof, verify_recursion_layer_proof};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        base_layer_proof_config,
        boojum::{
            algebraic_props::{
                round_function::AbsorptionModeOverwrite, sponge::GoldilocksPoseidon2Sponge,
            },
            cs::implementations::{
                pow::NoPow, proof::Proof, transcript::GoldilocksPoisedon2Transcript,
                verifier::VerificationKey, witness::WitnessVec,
            },
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
            worker::Worker,
        },
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerProof,
            recursion_layer::{ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType},
        },
        recursion_layer_proof_config,
    },
    CircuitWrapper, FriProofWrapper, ProverArtifacts, ProverJob, ProverServiceDataKey,
    WitnessVectorArtifacts,
};
use zksync_prover_keystore::GoldilocksGpuProverSetupData;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};
use zksync_utils::panic_extractor::try_extract_panic_message;

type DefaultTranscript = GoldilocksPoisedon2Transcript;
type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
pub type F = GoldilocksField;
pub type H = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
pub type Ext = GoldilocksExt2;

pub struct CircuitProver {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    receiver: Receiver<WitnessVectorArtifacts>,
    #[allow(dead_code)]
    prover_context: ProverContext,
    setup_keys: HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>,
}

impl CircuitProver {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        receiver: Receiver<WitnessVectorArtifacts>,
        max_allocation: Option<usize>,
        setup_keys: HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>,
    ) -> anyhow::Result<Self> {
        let prover_context = match max_allocation {
            Some(max_allocation) => ProverContext::create_with_config(
                ProverContextConfig::default().with_maximum_device_allocation(max_allocation),
            )
            .context("failed initializing gpu prover context")?,
            None => ProverContext::create().context("failed initializing gpu prover context")?,
        };
        Ok(Self {
            connection_pool,
            object_store,
            protocol_version,
            receiver,
            prover_context,
            setup_keys,
        })
    }

    pub async fn run(mut self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        while !cancellation_token.is_cancelled() {
            let mut get_vector_timer = Instant::now();

            let artifact = self
                .receiver
                .recv()
                .await
                .context("no witness vector generators are available")?;
            tracing::info!(
                "Prover received job after: {:?}",
                get_vector_timer.elapsed()
            );
            self.prove(artifact, cancellation_token.clone())
                .await
                .context("failed to prove circuit proof")?;

            get_vector_timer = Instant::now();
        }
        tracing::warn!("Stop signal received, shutting down Witness Vector Generator");
        Ok(())
    }

    async fn prove(
        &self,
        artifact: WitnessVectorArtifacts,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        let start = Instant::now();
        let block_number = artifact.prover_job.block_number;
        let setup_data_key = artifact.prover_job.setup_data_key.crypto_setup_key();
        let job_id = artifact.prover_job.job_id;
        let setup_data = self
            .setup_keys
            .get(&setup_data_key)
            .context("couldn't find key for setup_data_key")?
            .clone();
        let task = tokio::task::spawn_blocking(move || {
            let _span = tracing::info_span!("gpu_prove", %block_number).entered();
            Self::prove_circuit_proof(artifact, setup_data).context("failed to prove circuit")
        });

        self.wait_for_task(job_id, start, task, cancellation_token.clone())
            .await?;
        Ok(())
    }
    #[tracing::instrument(
        name = "Prover::prove",
        skip_all,
        fields(l1_batch = % witness_vector_artifacts.prover_job.block_number)
    )]
    pub fn prove_circuit_proof(
        witness_vector_artifacts: WitnessVectorArtifacts,
        setup_data: Arc<GoldilocksGpuProverSetupData>,
    ) -> anyhow::Result<ProverArtifacts> {
        let WitnessVectorArtifacts {
            witness_vector,
            prover_job,
        } = witness_vector_artifacts;

        let job_id = prover_job.job_id;
        let circuit_wrapper = prover_job.circuit_wrapper;
        let block_number = prover_job.block_number;

        let (proof, circuit_id) =
            Self::generate_proof(&circuit_wrapper, witness_vector, &setup_data)
                .context(format!("failed to generate proof for job id {job_id}"))?;

        Self::verify_proof(&circuit_wrapper, &proof, &setup_data.vk).context(format!(
            "failed to verify proof with job_id {job_id}, circuit_id: {circuit_id}"
        ))?;

        let proof_wrapper = match &circuit_wrapper {
            CircuitWrapper::Base(_) => {
                FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
            }
            CircuitWrapper::Recursive(_) => {
                FriProofWrapper::Recursive(ZkSyncRecursionLayerProof::from_inner(circuit_id, proof))
            }
            CircuitWrapper::BasePartial(_) => {
                return Err(anyhow::anyhow!(
                    "Unexpected partial base circuit for proving"
                ))
            }
        };
        Ok(ProverArtifacts::new(block_number, proof_wrapper))
    }

    fn generate_proof(
        circuit_wrapper: &CircuitWrapper,
        witness_vector: WitnessVec<GoldilocksField>,
        setup_data: &Arc<GoldilocksGpuProverSetupData>,
    ) -> anyhow::Result<(Proof<F, H, Ext>, u8)> {
        let worker = Worker::new();

        let (gpu_proof_config, proof_config, circuit_id) = match circuit_wrapper {
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
            CircuitWrapper::BasePartial(_) => {
                return Err(anyhow::anyhow!(
                    "Received unexpected partial base circuit for proving"
                ))
            }
        };

        let proof =
            gpu_prove_from_external_witness_data::<DefaultTranscript, DefaultTreeHasher, NoPow, _>(
                &gpu_proof_config,
                &witness_vector,
                proof_config,
                &setup_data.setup,
                &setup_data.vk,
                (),
                &worker,
            )
            .context("crypto primitive: failed to generate proof")?;
        Ok((proof.into(), circuit_id))
    }

    fn verify_proof(
        circuit_wrapper: &CircuitWrapper,
        proof: &Proof<F, H, Ext>,
        verification_key: &VerificationKey<F, H>,
    ) -> anyhow::Result<()> {
        let is_valid = match circuit_wrapper {
            CircuitWrapper::Base(base_circuit) => {
                verify_base_layer_proof::<NoPow>(base_circuit, proof, verification_key)
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                verify_recursion_layer_proof::<NoPow>(recursive_circuit, proof, verification_key)
            }
            CircuitWrapper::BasePartial(_) => {
                return Err(anyhow::anyhow!(
                    "received partial proof for verifying, unexpected"
                ))
            }
        };

        if !is_valid {
            return Err(anyhow::anyhow!("crypto primitive: failed to verify proof"));
        }
        Ok(())
    }

    async fn wait_for_task(
        &self,
        job_id: u32,
        started_at: Instant,
        task: JoinHandle<anyhow::Result<ProverArtifacts>>,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                tracing::info!("received cancellation!");
                return Ok(())
            }
            result = task => {
                let error_message = match result {
                    Ok(Ok(prover_artifact)) => {
                        self
                            .save_result(job_id, started_at, prover_artifact)
                            .await.context("failed to save result")?;
                        tracing::info!("Prover executed job in: {:?}", started_at.elapsed());
                        return Ok(())
                    }
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => try_extract_panic_message(error),
                };
                tracing::error!(
                    "Error occurred while processing {} job {:?}: {:?}",
                    "prover",
                    job_id,
                    error_message
                );
                self.save_failure(job_id, error_message).await.context("failed to save result")?;
            }
        }

        Ok(())
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: ProverArtifacts,
    ) -> anyhow::Result<()> {
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to get connection")?;
        let proof = artifacts.proof_wrapper;

        let (_circuit_type, is_scheduler_proof) = match &proof {
            FriProofWrapper::Base(base) => (base.numeric_circuit_type(), false),
            FriProofWrapper::Recursive(recursive_circuit) => match recursive_circuit {
                ZkSyncRecursionLayerProof::SchedulerCircuit(_) => {
                    (recursive_circuit.numeric_circuit_type(), true)
                }
                _ => (recursive_circuit.numeric_circuit_type(), false),
            },
        };

        // let blob_save_started_at = Instant::now();
        let blob_url = self
            .object_store
            .put(job_id, &proof)
            .await
            .context("failed to upload to object store")?;

        // METRICS.blob_save_time[&circuit_type.to_string()].observe(blob_save_started_at.elapsed());

        let mut transaction = connection
            .start_transaction()
            .await
            .context("failed to start transaction")?;
        transaction
            .fri_prover_jobs_dal()
            .save_proof(job_id, started_at.elapsed(), &blob_url)
            .await;
        if is_scheduler_proof {
            transaction
                .fri_proof_compressor_dal()
                .insert_proof_compression_job(
                    artifacts.block_number,
                    &blob_url,
                    self.protocol_version,
                )
                .await;
        }
        transaction
            .commit()
            .await
            .context("failed to commit transaction")?;
        Ok(())
    }

    async fn save_failure(&self, job_id: u32, error: String) -> anyhow::Result<()> {
        Ok(self
            .connection_pool
            .connection()
            .await
            .context("failed to get db connection")?
            .fri_prover_jobs_dal()
            .save_proof_error(job_id, error)
            .await)
    }
}
