use std::{sync::Arc, time::Instant};

use anyhow::Context;
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
            cs::implementations::{pow::NoPow, witness::WitnessVec},
            field::goldilocks::GoldilocksField,
            worker::Worker,
        },
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerProof, recursion_layer::ZkSyncRecursionLayerProof,
        },
        recursion_layer_proof_config,
    },
    CircuitWrapper, FriProofWrapper, ProverArtifacts, WitnessVectorArtifactsTemp,
};
use zksync_prover_keystore::GoldilocksGpuProverSetupData;
use zksync_types::protocol_version::ProtocolSemanticVersion;
use zksync_utils::panic_extractor::try_extract_panic_message;

use crate::{
    metrics::CIRCUIT_PROVER_METRICS,
    types::{DefaultTranscript, DefaultTreeHasher, Proof, VerificationKey},
    SetupDataCache,
};

/// In charge of proving circuits, given a Witness Vector source.
/// Both job runner & job executor.
#[derive(Debug)]
pub struct CircuitProver {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    /// Witness Vector source receiver
    receiver: Receiver<WitnessVectorArtifactsTemp>,
    /// Setup Data used for proving & proof verification
    setup_data_cache: SetupDataCache,
}

impl CircuitProver {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        receiver: Receiver<WitnessVectorArtifactsTemp>,
        max_allocation: Option<usize>,
        setup_data_cache: SetupDataCache,
    ) -> anyhow::Result<(Self, ProverContext)> {
        // VRAM allocation
        let prover_context = match max_allocation {
            Some(max_allocation) => ProverContext::create_with_config(
                ProverContextConfig::default().with_maximum_device_allocation(max_allocation),
            )
            .context("failed initializing fixed gpu prover context")?,
            None => ProverContext::create().context("failed initializing gpu prover context")?,
        };
        Ok((
            Self {
                connection_pool,
                object_store,
                protocol_version,
                receiver,
                setup_data_cache,
            },
            prover_context,
        ))
    }

    /// Continuously polls `receiver` for Witness Vectors and proves them.
    /// All job executions are persisted.
    pub async fn run(mut self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        while !cancellation_token.is_cancelled() {
            let time = Instant::now();

            let artifact = self
                .receiver
                .recv()
                .await
                .context("no Witness Vector Generators are available")?;
            tracing::info!(
                "Circuit Prover received job {:?} after: {:?}",
                artifact.prover_job.job_id,
                time.elapsed()
            );
            CIRCUIT_PROVER_METRICS.job_wait_time.observe(time.elapsed());

            self.prove(artifact, cancellation_token.clone())
                .await
                .context("failed to prove circuit proof")?;
        }
        tracing::info!("Circuit Prover shut down.");
        Ok(())
    }

    /// Proves a job, with persistence of execution.
    async fn prove(
        &self,
        artifact: WitnessVectorArtifactsTemp,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        let time = Instant::now();
        let block_number = artifact.prover_job.block_number;
        let job_id = artifact.prover_job.job_id;
        let job_start_time = artifact.time;
        let setup_data_key = artifact.prover_job.setup_data_key.crypto_setup_key();
        let setup_data = self
            .setup_data_cache
            .get(&setup_data_key)
            .context(format!(
                "failed to get setup data for key {setup_data_key:?}"
            ))?
            .clone();
        let task = tokio::task::spawn_blocking(move || {
            let _span = tracing::info_span!("prove_circuit_proof", %block_number).entered();
            Self::prove_circuit_proof(artifact, setup_data).context("failed to prove circuit")
        });

        self.finish_task(
            job_id,
            time,
            job_start_time,
            task,
            cancellation_token.clone(),
        )
        .await?;
        tracing::info!(
            "Circuit Prover finished job {:?} in: {:?}",
            job_id,
            time.elapsed()
        );
        CIRCUIT_PROVER_METRICS
            .job_finished_time
            .observe(time.elapsed());
        CIRCUIT_PROVER_METRICS
            .full_proving_time
            .observe(job_start_time.elapsed());
        Ok(())
    }

    /// Proves a job using crypto primitives (proof generation & proof verification).
    #[tracing::instrument(
        name = "Prover::prove_circuit_proof",
        skip_all,
        fields(l1_batch = % witness_vector_artifacts.prover_job.block_number)
    )]
    pub fn prove_circuit_proof(
        witness_vector_artifacts: WitnessVectorArtifactsTemp,
        setup_data: Arc<GoldilocksGpuProverSetupData>,
    ) -> anyhow::Result<ProverArtifacts> {
        let time = Instant::now();
        let WitnessVectorArtifactsTemp {
            witness_vector,
            prover_job,
            ..
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
                return Self::partial_proof_error();
            }
        };
        CIRCUIT_PROVER_METRICS
            .crypto_primitives_time
            .observe(time.elapsed());
        Ok(ProverArtifacts::new(block_number, proof_wrapper))
    }

    /// Generates a proof from crypto primitives.
    fn generate_proof(
        circuit_wrapper: &CircuitWrapper,
        witness_vector: WitnessVec<GoldilocksField>,
        setup_data: &Arc<GoldilocksGpuProverSetupData>,
    ) -> anyhow::Result<(Proof, u8)> {
        let time = Instant::now();

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
                return Self::partial_proof_error();
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
        CIRCUIT_PROVER_METRICS
            .generate_proof_time
            .observe(time.elapsed());
        Ok((proof.into(), circuit_id))
    }

    /// Verifies a proof from crypto primitives
    fn verify_proof(
        circuit_wrapper: &CircuitWrapper,
        proof: &Proof,
        verification_key: &VerificationKey,
    ) -> anyhow::Result<()> {
        let time = Instant::now();

        let is_valid = match circuit_wrapper {
            CircuitWrapper::Base(base_circuit) => {
                verify_base_layer_proof::<NoPow>(base_circuit, proof, verification_key)
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                verify_recursion_layer_proof::<NoPow>(recursive_circuit, proof, verification_key)
            }
            CircuitWrapper::BasePartial(_) => {
                return Self::partial_proof_error();
            }
        };

        CIRCUIT_PROVER_METRICS
            .verify_proof_time
            .observe(time.elapsed());

        if !is_valid {
            return Err(anyhow::anyhow!("crypto primitive: failed to verify proof"));
        }
        Ok(())
    }

    /// This code path should never trigger. All proofs are hydrated during Witness Vector Generator.
    /// If this triggers, it means that proof hydration in Witness Vector Generator was not done -- logic bug.
    fn partial_proof_error<T>() -> anyhow::Result<T> {
        Err(anyhow::anyhow!("received unexpected dehydrated proof"))
    }

    /// Runs task to completion and persists result.
    /// NOTE: Task may be cancelled mid-flight.
    async fn finish_task(
        &self,
        job_id: u32,
        time: Instant,
        job_start_time: Instant,
        task: JoinHandle<anyhow::Result<ProverArtifacts>>,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                tracing::info!("Stop signal received, shutting down Circuit Prover...");
                return Ok(())
            }
            result = task => {
                let error_message = match result {
                    Ok(Ok(prover_artifact)) => {
                        tracing::info!("Circuit Prover executed job {:?} in: {:?}", job_id, time.elapsed());
                        CIRCUIT_PROVER_METRICS.execution_time.observe(time.elapsed());
                        self
                            .save_result(job_id, job_start_time, prover_artifact)
                            .await.context("failed to save result")?;
                        return Ok(())
                    }
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => try_extract_panic_message(error),
                };
                tracing::error!(
                    "Circuit Prover failed on job {:?} with error {:?}",
                    job_id,
                    error_message
                );

                self.save_failure(job_id, error_message).await.context("failed to save failure")?;
            }
        }

        Ok(())
    }

    /// Persists proof generated.
    /// Job metadata is saved to database, whilst artifacts go to object store.
    async fn save_result(
        &self,
        job_id: u32,
        job_start_time: Instant,
        artifacts: ProverArtifacts,
    ) -> anyhow::Result<()> {
        let time = Instant::now();
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .context("failed to get db connection")?;
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

        let upload_time = Instant::now();
        let blob_url = self
            .object_store
            .put(job_id, &proof)
            .await
            .context("failed to upload to object store")?;
        CIRCUIT_PROVER_METRICS
            .artifact_upload_time
            .observe(upload_time.elapsed());

        let mut transaction = connection
            .start_transaction()
            .await
            .context("failed to start db transaction")?;
        transaction
            .fri_prover_jobs_dal()
            .save_proof(job_id, job_start_time.elapsed(), &blob_url)
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
            .context("failed to commit db transaction")?;

        tracing::info!(
            "Circuit Prover saved job {:?} after {:?}",
            job_id,
            time.elapsed()
        );
        CIRCUIT_PROVER_METRICS.save_time.observe(time.elapsed());

        Ok(())
    }

    /// Persists job execution error to database.
    async fn save_failure(&self, job_id: u32, error: String) -> anyhow::Result<()> {
        self.connection_pool
            .connection()
            .await
            .context("failed to get db connection")?
            .fri_prover_jobs_dal()
            .save_proof_error(job_id, error)
            .await;
        Ok(())
    }
}
