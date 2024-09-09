use std::{
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
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion, L1BatchNumber,
};
use zksync_utils::panic_extractor::try_extract_panic_message;

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
    CircuitWrapper, FriProofWrapper, ProverJob, ProverServiceDataKey, WitnessVectorArtifacts,
};
use zksync_prover_keystore::{keystore::Keystore, GoldilocksGpuProverSetupData};

type DefaultTranscript = GoldilocksPoisedon2Transcript;
type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;

pub struct CircuitProver {
    connection_pool: ConnectionPool<Prover>,
    object_store: Arc<dyn ObjectStore>,
    protocol_version: ProtocolSemanticVersion,
    keystore: Keystore,
    receiver: Receiver<WitnessVectorArtifacts>,
    prover_context: ProverContext,
}

impl CircuitProver {
    pub fn new(
        connection_pool: ConnectionPool<Prover>,
        object_store: Arc<dyn ObjectStore>,
        protocol_version: ProtocolSemanticVersion,
        keystore: Keystore,
        receiver: Receiver<WitnessVectorArtifacts>,
        max_allocation: Option<usize>,
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
            keystore,
            receiver,
            prover_context,
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

        let started_at = Instant::now();
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
        while !cancellation_token.is_cancelled() {
            if let Some((job_id, job)) = self
                .get_next_job()
                .await
                .context("failed during get_next_job()")?
            // .context("failed during get_next_job()")?
            {
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
                                continue;
                                    // .context("save_result()");
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
        let now = Instant::now();
        // tracing::info!("Attempting to get new job from assembly queue.");
        // let mut queue = self.receiver.recv().await;
        // let is_full = queue.is_full();
        // tracing::info!(
        //         "Queue has {} items with max capacity {}. Queue is_full = {}.",
        //         queue.size(),
        //         queue.capacity(),
        //         is_full
        //     );
        let job = match self.receiver.recv().await {
            None => {
                tracing::error!("No producers available");
                Err(anyhow!("No producer available"))
            }
            Some(witness_vector) => Ok(Some((witness_vector.prover_job.job_id, witness_vector))),
        };
        tracing::info!("Received job after {:?}", now.elapsed());
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
        // save_proof(
        //     job_id,
        //     started_at,
        //     artifacts,
        //     &*self.blob_store,
        //     self.public_blob_store.as_deref(),
        //     self.config.shall_save_to_public_bucket,
        //     &mut storage_processor,
        //     self.protocol_version,
        // )
        //     .await;
        // #[allow(clippy::too_many_arguments)]
        // pub async fn save_proof(
        //     job_id: u32,
        //     started_at: Instant,
        //     artifacts: ProverArtifacts,
        //     blob_store: &dyn ObjectStore,
        //     public_blob_store: Option<&dyn ObjectStore>,
        //     shall_save_to_public_bucket: bool,
        //     connection: &mut Connection<'_, Prover>,
        //     protocol_version: ProtocolSemanticVersion,
        // ) {
        // tracing::info!(
        //     "Successfully proven job: {}, total time taken: {:?}",
        //     job_id,
        //     started_at.elapsed()
        // );
        let proof = artifacts.proof_wrapper;

        // We save the scheduler proofs in public bucket,
        // so that it can be verified independently while we're doing shadow proving
        let (circuit_type, is_scheduler_proof) = match &proof {
            FriProofWrapper::Base(base) => (base.numeric_circuit_type(), false),
            FriProofWrapper::Recursive(recursive_circuit) => match recursive_circuit {
                ZkSyncRecursionLayerProof::SchedulerCircuit(_) => {
                    (recursive_circuit.numeric_circuit_type(), true)
                }
                _ => (recursive_circuit.numeric_circuit_type(), false),
            },
        };

        let blob_save_started_at = Instant::now();
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

        let started_at = Instant::now();
        // let keystore = self.keystore.clone();
        let artifact: GoldilocksGpuProverSetupData = self
            .keystore
            .load_gpu_setup_data_for_circuit_type(key.clone())
            .context("load_gpu_setup_data_for_circuit_type()")?;

        // METRICS.gpu_setup_data_load_time[&key.circuit_id.to_string()]
        //     .observe(started_at.elapsed());

        Ok(Arc::new(artifact))
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
    let started_at = Instant::now();
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

// use std::{collections::HashMap, sync::Arc, time::Instant};
//
// use anyhow::Context as _;
// use shivini::{
//     gpu_proof_config::GpuProofConfig, gpu_prove_from_external_witness_data, ProverContext,
// };
// use tokio::task::JoinHandle;
// use zksync_config::configs::{fri_prover_group::FriProverGroupConfig, FriProverConfig};
// use zksync_env_config::FromEnv;
// use zksync_object_store::ObjectStore;
// use zksync_queued_job_processor::{async_trait, JobProcessor};
// use zksync_types::{
//     basic_fri_types::CircuitIdRoundTuple, protocol_version::ProtocolSemanticVersion,
//     prover_dal::SocketAddress,
// };
//
// use zksync_prover_dal::{ConnectionPool, ProverDal};
// use zksync_prover_fri_types::{
//     circuit_definitions::{
//         base_layer_proof_config,
//         boojum::{
//             algebraic_props::{
//                 round_function::AbsorptionModeOverwrite, sponge::GoldilocksPoseidon2Sponge,
//             },
//             cs::implementations::{pow::NoPow, transcript::GoldilocksPoisedon2Transcript},
//             worker::Worker,
//         },
//         circuit_definitions::{
//             base_layer::ZkSyncBaseLayerProof, recursion_layer::ZkSyncRecursionLayerProof,
//         },
//         recursion_layer_proof_config,
//     },
//     CircuitWrapper, FriProofWrapper, ProverServiceDataKey, WitnessVectorArtifacts,
// };
// use zksync_prover_fri_utils::region_fetcher::Zone;
// use zksync_vk_setup_data_server_fri::{GoldilocksGpuProverSetupData, keystore::Keystore};
//
// use crate::{
//     // metrics::METRICS,
//     utils::{
//         get_setup_data_key, GpuProverJob, ProverArtifacts, save_proof,
//         setup_metadata_to_setup_data_key, SharedWitnessVectorQueue, verify_proof,
//     },
// };
//
// type DefaultTranscript = GoldilocksPoisedon2Transcript;
// type DefaultTreeHasher = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
//
// pub enum SetupLoadMode {
//     FromMemory(HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>),
//     FromDisk,
// }
//
// #[allow(dead_code)]
// pub struct Prover {
//     blob_store: Arc<dyn ObjectStore>,
//     public_blob_store: Option<Arc<dyn ObjectStore>>,
//     config: Arc<FriProverConfig>,
//     prover_connection_pool: ConnectionPool<zksync_prover_dal::Prover>,
//     setup_load_mode: SetupLoadMode,
//     // Only pick jobs for the configured circuit id and aggregation rounds.
//     // Empty means all jobs are picked.
//     circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
//     witness_vector_queue: SharedWitnessVectorQueue,
//     prover_context: ProverContext,
//     address: SocketAddress,
//     zone: Zone,
//     protocol_version: ProtocolSemanticVersion,
// }
//
// impl Prover {
//     #[allow(dead_code)]
//     pub fn new(
//         blob_store: Arc<dyn ObjectStore>,
//         public_blob_store: Option<Arc<dyn ObjectStore>>,
//         config: FriProverConfig,
//         prover_connection_pool: ConnectionPool<zksync_prover_dal::Prover>,
//         setup_load_mode: SetupLoadMode,
//         circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
//         witness_vector_queue: SharedWitnessVectorQueue,
//         address: SocketAddress,
//         zone: Zone,
//         protocol_version: ProtocolSemanticVersion,
//     ) -> Self {
//         Prover {
//             blob_store,
//             public_blob_store,
//             config: Arc::new(config),
//             prover_connection_pool,
//             setup_load_mode,
//             circuit_ids_for_round_to_be_proven,
//             witness_vector_queue,
//             prover_context: ProverContext::create()
//                 .expect("failed initializing gpu prover context"),
//             address,
//             zone,
//             protocol_version,
//         }
//     }
//
//     #[tracing::instrument(name = "Prover::get_setup_data", skip_all)]
//     fn get_setup_data(
//         &self,
//         key: ProverServiceDataKey,
//     ) -> anyhow::Result<Arc<GoldilocksGpuProverSetupData>> {
//         let key = get_setup_data_key(key);
//         Ok(match &self.setup_load_mode {
//             SetupLoadMode::FromMemory(cache) => cache
//                 .get(&key)
//                 .context("Setup data not found in cache")?
//                 .clone(),
//             SetupLoadMode::FromDisk => {
//                 let started_at = Instant::now();
//                 let keystore =
//                     Keystore::new_with_setup_data_path(self.config.setup_data_path.clone());
//                 let artifact: GoldilocksGpuProverSetupData = keystore
//                     .load_gpu_setup_data_for_circuit_type(key.clone())
//                     .context("load_gpu_setup_data_for_circuit_type()")?;
//
//                 // METRICS.gpu_setup_data_load_time[&key.circuit_id.to_string()]
//                 //     .observe(started_at.elapsed());
//
//                 Arc::new(artifact)
//             }
//         })
//     }
//
//     #[tracing::instrument(
//         name = "Prover::prove",
//         skip_all,
//         fields(l1_batch = % job.witness_vector_artifacts.prover_job.block_number)
//     )]
//     pub fn prove(
//         job: GpuProverJob,
//         setup_data: Arc<GoldilocksGpuProverSetupData>,
//     ) -> ProverArtifacts {
//         let worker = Worker::new();
//         let GpuProverJob {
//             witness_vector_artifacts,
//         } = job;
//         let WitnessVectorArtifacts {
//             witness_vector,
//             prover_job,
//         } = witness_vector_artifacts;
//
//         let (gpu_proof_config, proof_config, circuit_id) = match &prover_job.circuit_wrapper {
//             CircuitWrapper::Base(circuit) => (
//                 GpuProofConfig::from_base_layer_circuit(circuit),
//                 base_layer_proof_config(),
//                 circuit.numeric_circuit_type(),
//             ),
//             CircuitWrapper::Recursive(circuit) => (
//                 GpuProofConfig::from_recursive_layer_circuit(circuit),
//                 recursion_layer_proof_config(),
//                 circuit.numeric_circuit_type(),
//             ),
//             CircuitWrapper::BasePartial(_) => panic!("Invalid CircuitWrapper received"),
//         };
//
//         let started_at = Instant::now();
//         let proof = gpu_prove_from_external_witness_data::<
//             DefaultTranscript,
//             DefaultTreeHasher,
//             NoPow,
//             _,
//         >(
//             &gpu_proof_config,
//             &witness_vector,
//             proof_config,
//             &setup_data.setup,
//             &setup_data.vk,
//             (),
//             &worker,
//         )
//             .unwrap_or_else(|_| {
//                 panic!("failed generating GPU proof for id: {}", prover_job.job_id)
//             });
//         tracing::info!(
//             "Successfully generated gpu proof for job {} took: {:?}",
//             prover_job.job_id,
//             started_at.elapsed()
//         );
//         // METRICS.gpu_proof_generation_time[&circuit_id.to_string()]
//         //     .observe(started_at.elapsed());
//
//         let proof = proof.into();
//         verify_proof(
//             &prover_job.circuit_wrapper,
//             &proof,
//             &setup_data.vk,
//             prover_job.job_id,
//         );
//         let proof_wrapper = match &prover_job.circuit_wrapper {
//             CircuitWrapper::Base(_) => {
//                 FriProofWrapper::Base(ZkSyncBaseLayerProof::from_inner(circuit_id, proof))
//             }
//             CircuitWrapper::Recursive(_) => FriProofWrapper::Recursive(
//                 ZkSyncRecursionLayerProof::from_inner(circuit_id, proof),
//             ),
//             CircuitWrapper::BasePartial(_) => panic!("Received partial base circuit"),
//         };
//         ProverArtifacts::new(prover_job.block_number, proof_wrapper)
//     }
// }
// #[async_trait]
// impl JobProcessor for Prover {
//     type Job = GpuProverJob;
//     type JobId = u32;
//     type JobArtifacts = ProverArtifacts;
//
//     // we use smaller number here as the polling in done from the in-memory queue not DB
//     const POLLING_INTERVAL_MS: u64 = 200;
//     const MAX_BACKOFF_MS: u64 = 1_000;
//     const SERVICE_NAME: &'static str = "FriGpuProver";
//
//     async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
//         let now = Instant::now();
//         tracing::info!("Attempting to get new job from assembly queue.");
//         let mut queue = self.witness_vector_queue.lock().await;
//         let is_full = queue.is_full();
//         tracing::info!(
//             "Queue has {} items with max capacity {}. Queue is_full = {}.",
//             queue.size(),
//             queue.capacity(),
//             is_full
//         );
//         match queue.remove() {
//             Err(_) => {
//                 tracing::warn!("No assembly available in queue after {:?}.", now.elapsed());
//                 Ok(None)
//             }
//             Ok(item) => {
//                 if is_full {
//                     self.prover_connection_pool
//                         .connection()
//                         .await
//                         .unwrap()
//                         .fri_gpu_prover_queue_dal()
//                         .update_prover_instance_from_full_to_available(
//                             self.address.clone(),
//                             self.zone.to_string(),
//                         )
//                         .await;
//                 }
//                 tracing::info!(
//                     "Assembly received after {:?}. Starting GPU proving for job: {:?}",
//                     now.elapsed(),
//                     item.witness_vector_artifacts.prover_job.job_id
//                 );
//                 Ok(Some((
//                     item.witness_vector_artifacts.prover_job.job_id,
//                     item,
//                 )))
//             }
//         }
//     }
//
//     async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
//         self.prover_connection_pool
//             .connection()
//             .await
//             .unwrap()
//             .fri_prover_jobs_dal()
//             .save_proof_error(job_id, error)
//             .await;
//     }
//
//     async fn process_job(
//         &self,
//         _job_id: &Self::JobId,
//         job: Self::Job,
//         _started_at: Instant,
//     ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
//         let setup_data = self.get_setup_data(
//             job.witness_vector_artifacts
//                 .prover_job
//                 .setup_data_key
//                 .clone(),
//         );
//         tokio::task::spawn_blocking(move || {
//             let block_number = job.witness_vector_artifacts.prover_job.block_number;
//             let _span = tracing::info_span!("gpu_prove", %block_number).entered();
//             Ok(Self::prove(job, setup_data.context("get_setup_data()")?))
//         })
//     }
//
//     async fn save_result(
//         &self,
//         job_id: Self::JobId,
//         started_at: Instant,
//         artifacts: Self::JobArtifacts,
//     ) -> anyhow::Result<()> {
//         // METRICS.gpu_total_proving_time.observe(started_at.elapsed());
//
//         let mut storage_processor = self.prover_connection_pool.connection().await.unwrap();
//         save_proof(
//             job_id,
//             started_at,
//             artifacts,
//             &*self.blob_store,
//             self.public_blob_store.as_deref(),
//             self.config.shall_save_to_public_bucket,
//             &mut storage_processor,
//             self.protocol_version,
//         )
//             .await;
//         Ok(())
//     }
//
//     fn max_attempts(&self) -> u32 {
//         self.config.max_attempts
//     }
//
//     async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
//         let mut prover_storage = self
//             .prover_connection_pool
//             .connection()
//             .await
//             .context("failed to acquire DB connection for Prover")?;
//         prover_storage
//             .fri_prover_jobs_dal()
//             .get_prover_job_attempts(*job_id)
//             .await
//             .map(|attempts| attempts.unwrap_or(0))
//             .context("failed to get job attempts for Prover")
//     }
// }
//
// pub fn load_setup_data_cache(config: &FriProverConfig) -> anyhow::Result<SetupLoadMode> {
//     Ok(match config.setup_load_mode {
//         zksync_config::configs::fri_prover::SetupLoadMode::FromDisk => SetupLoadMode::FromDisk,
//         zksync_config::configs::fri_prover::SetupLoadMode::FromMemory => {
//             let mut cache = HashMap::new();
//             tracing::info!(
//                 "Loading setup data cache for group {}",
//                 &config.specialized_group_id
//             );
//             let prover_setup_metadata_list = FriProverGroupConfig::from_env()
//                 .context("FriProverGroupConfig::from_env()")?
//                 .get_circuit_ids_for_group_id(config.specialized_group_id)
//                 .context(
//                     "At least one circuit should be configured for group when running in FromMemory mode",
//                 )?;
//             tracing::info!(
//                 "for group {} configured setup metadata are {:?}",
//                 &config.specialized_group_id,
//                 prover_setup_metadata_list
//             );
//             let keystore = Keystore::new_with_setup_data_path(config.setup_data_path.clone());
//             for prover_setup_metadata in prover_setup_metadata_list {
//                 let key = setup_metadata_to_setup_data_key(&prover_setup_metadata);
//                 let setup_data = keystore
//                     .load_gpu_setup_data_for_circuit_type(key.clone())
//                     .context("load_gpu_setup_data_for_circuit_type()")?;
//                 cache.insert(key, Arc::new(setup_data));
//             }
//             SetupLoadMode::FromMemory(cache)
//         }
//     })
// }
//
