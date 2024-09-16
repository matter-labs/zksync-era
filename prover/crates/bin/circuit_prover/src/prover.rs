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
                verifier::VerificationKey,
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
    CircuitWrapper, FriProofWrapper, ProverServiceDataKey, WitnessVectorArtifacts,
};
use zksync_prover_keystore::GoldilocksGpuProverSetupData;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};
use zksync_utils::panic_extractor::try_extract_panic_message;

type DefaultTranscript = GoldilocksPoisedon2Transcript;
type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

pub struct CircuitProver {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    receiver: Receiver<WitnessVectorArtifacts>,
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
    ) -> Self {
        let prover_context = match max_allocation {
            Some(max_allocation) => ProverContext::create_with_config(
                ProverContextConfig::default().with_maximum_device_allocation(max_allocation),
            )
            .expect("failed initializing gpu prover context"),
            None => ProverContext::create().expect("failed initializing gpu prover context"),
        };
        Self {
            connection_pool,
            object_store,
            protocol_version,
            receiver,
            prover_context,
            setup_keys,
        }
    }

    const POLLING_INTERVAL_MS: u64 = 1500;

    #[tracing::instrument(
        name = "Prover::prove",
        skip_all,
        fields(l1_batch = % witness_vector_artifacts.prover_job.block_number)
    )]
    pub fn prove(
        witness_vector_artifacts: WitnessVectorArtifacts,
        setup_data: Arc<GoldilocksGpuProverSetupData>,
    ) -> ProverArtifacts {
        let worker = Worker::new();

        let WitnessVectorArtifacts {
            witness_vector,
            prover_job,
        } = witness_vector_artifacts;

        let (gpu_proof_config, proof_config, circuit_id) = match &prover_job.circuit_wrapper {
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
            CircuitWrapper::BasePartial(_) => panic!("Invalid CircuitWrapper received"),
        };

        let _started_at = Instant::now();
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
            .unwrap_or_else(|_| {
                panic!("failed generating GPU proof for id: {}", prover_job.job_id)
            });
        // tracing::info!(
        //     "Successfully generated gpu proof for job {} took: {:?}",
        //     prover_job.job_id,
        //     started_at.elapsed()
        // );
        // METRICS.gpu_proof_generation_time[&circuit_id.to_string()]
        //     .observe(started_at.elapsed());

        let proof = proof.into();
        verify_proof(
            &prover_job.circuit_wrapper,
            &proof,
            &setup_data.vk,
            prover_job.job_id,
        );
        let proof_wrapper = match &prover_job.circuit_wrapper {
            CircuitWrapper::Base(_) => {
                FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
            }
            CircuitWrapper::Recursive(_) => {
                FriProofWrapper::Recursive(ZkSyncRecursionLayerProof::from_inner(circuit_id, proof))
            }
            CircuitWrapper::BasePartial(_) => panic!("Received partial base circuit"),
        };
        ProverArtifacts::new(prover_job.block_number, proof_wrapper)
    }

    pub async fn run(mut self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        let mut start = Instant::now();
        while !cancellation_token.is_cancelled() {
            if let Some((job_id, job)) = self
                .get_next_job()
                .await
                .context("failed during get_next_job()")?
            {
                tracing::info!("Prover received job after: {:?}", start.elapsed());
                let started_at = Instant::now();
                backoff = Self::POLLING_INTERVAL_MS;
                // tracing::debug!(
                //                     "Spawning thread processing {:?} job with id {:?}",
                //                     Self::SERVICE_NAME,
                //                     job_id
                //                 );
                let task = self.process_job(job_id, job, started_at).await;

                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        tracing::info!("received cancellation!");
                        return Ok(())
                    }
                    result = task => {
                        let error_message = match result {
                            Ok(Ok(data)) => {
                                // tracing::info!(
                                //     "{} Job {:?} finished successfully",
                                //     "witness_vector_generator",
                                //     job_id
                                // );
                // METRICS.attempts[&Self::SERVICE_NAME].observe(attempts as usize);
                                self
                                    .save_result(job_id, started_at, data)
                                    .await;
                                                tracing::info!("Prover executed job in: {:?}", started_at.elapsed());
                start = Instant::now();
                                continue;
                            }
                            Ok(Err(error)) => error.to_string(),
                            Err(error) => try_extract_panic_message(error),
                        };
                        tracing::error!(
                            "Error occurred while processing {} job {:?}: {:?}",
                            "witness_vector_generator",
                            job_id,
                            error_message
                        );

                        self.save_failure(job_id, started_at, error_message).await;
                    }
                }
                tracing::info!("Prover executed job in: {:?}", started_at.elapsed());
                start = Instant::now();
                continue;
            };
            tracing::info!("Backing off for {} ms", backoff);
            // Error here corresponds to a timeout w/o receiving task cancel; we're OK with this.
            tokio::time::timeout(
                Duration::from_millis(backoff),
                cancellation_token.cancelled(),
            )
            .await
            .ok();
            const MAX_BACKOFF_MS: u64 = 60_000;
            const BACKOFF_MULTIPLIER: u64 = 2;
            backoff = (backoff * BACKOFF_MULTIPLIER).min(MAX_BACKOFF_MS);
        }
        tracing::warn!("Stop signal received, shutting down Witness Vector Generator");
        Ok(())
    }

    async fn get_next_job(&mut self) -> anyhow::Result<Option<(u32, WitnessVectorArtifacts)>> {
        // let now = Instant::now();
        let job = match self.receiver.recv().await {
            None => {
                tracing::error!("No producers available");
                Err(anyhow!("No producer available"))
            }
            Some(witness_vector) => Ok(Some((witness_vector.prover_job.job_id, witness_vector))),
        };
        // tracing::info!("Received job after {:?}", now.elapsed());
        job
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) {
        self.connection_pool
            .connection()
            .await
            .unwrap()
            .fri_prover_jobs_dal()
            .save_proof_error(job_id, error)
            .await;
    }

    async fn process_job(
        &self,
        _job_id: u32,
        witness_vector: WitnessVectorArtifacts,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<ProverArtifacts>> {
        let setup_data = self.get_setup_data(witness_vector.prover_job.setup_data_key.clone());
        tokio::task::spawn_blocking(move || {
            let block_number = witness_vector.prover_job.block_number;
            let _span = tracing::info_span!("gpu_prove", %block_number).entered();
            Ok(Self::prove(
                witness_vector,
                setup_data.context("get_setup_data()")?,
            ))
        })
    }

    async fn save_result(&self, job_id: u32, started_at: Instant, artifacts: ProverArtifacts) {
        // METRICS.gpu_total_proving_time.observe(started_at.elapsed());

        let mut connection = self.connection_pool.connection().await.unwrap();
        let proof = artifacts.proof_wrapper;

        // We save the scheduler proofs in public bucket,
        // so that it can be verified independently while we're doing shadow proving
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
        let blob_url = self.object_store.put(job_id, &proof).await.unwrap();

        // METRICS.blob_save_time[&circuit_type.to_string()].observe(blob_save_started_at.elapsed());

        let mut transaction = connection.start_transaction().await.unwrap();
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
        transaction.commit().await.unwrap();
    }
    #[tracing::instrument(name = "Prover::get_setup_data", skip_all)]
    fn get_setup_data(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<Arc<GoldilocksGpuProverSetupData>> {
        let key = get_setup_data_key(key);
        Ok(self.setup_keys.get(&key).unwrap().clone())
    }
}

pub struct ProverArtifacts {
    block_number: L1BatchNumber,
    pub proof_wrapper: FriProofWrapper,
}

impl ProverArtifacts {
    pub fn new(block_number: L1BatchNumber, proof_wrapper: FriProofWrapper) -> Self {
        Self {
            block_number,
            proof_wrapper,
        }
    }
}

pub type F = GoldilocksField;
pub type H = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
pub type Ext = GoldilocksExt2;
pub fn verify_proof(
    circuit_wrapper: &CircuitWrapper,
    proof: &Proof<F, H, Ext>,
    vk: &VerificationKey<F, H>,
    job_id: u32,
) {
    // let started_at = Instant::now();
    let (is_valid, circuit_id) = match circuit_wrapper {
        CircuitWrapper::Base(base_circuit) => (
            verify_base_layer_proof::<NoPow>(base_circuit, proof, vk),
            base_circuit.numeric_circuit_type(),
        ),
        CircuitWrapper::Recursive(recursive_circuit) => (
            verify_recursion_layer_proof::<NoPow>(recursive_circuit, proof, vk),
            recursive_circuit.numeric_circuit_type(),
        ),
        CircuitWrapper::BasePartial(_) => panic!("Invalid CircuitWrapper received"),
    };

    // METRICS.proof_verification_time[&circuit_id.to_string()].observe(started_at.elapsed());

    if !is_valid {
        let msg = format!("Failed to verify proof for job-id: {job_id} circuit_type {circuit_id}");
        tracing::error!("{}", msg);
        panic!("{}", msg);
    }
}

pub fn get_setup_data_key(key: ProverServiceDataKey) -> ProverServiceDataKey {
    match key.round {
        AggregationRound::NodeAggregation => {
            // For node aggregation only one key exist for all circuit types
            ProverServiceDataKey {
                circuit_id: ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
                round: key.round,
            }
        }
        _ => key,
    }
}
