use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            cs::implementations::setup::FinalizationHintsForProver,
            field::goldilocks::GoldilocksField,
            gadgets::queue::full_state_queue::FullStateCircuitQueueRawWitness,
        },
        circuit_definitions::base_layer::ZkSyncBaseLayerCircuit,
    },
    get_current_pod_name,
    keys::{FriCircuitKey, RamPermutationQueueWitnessKey},
    CircuitWrapper, ProverJob, ProverServiceDataKey, RamPermutationQueueWitness,
    WitnessVectorArtifacts,
};
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::{
    basic_fri_types::CircuitIdRoundTuple, protocol_version::ProtocolSemanticVersion,
};
use zksync_utils::panic_extractor::try_extract_panic_message;

pub struct WitnessVectorGenerator {
    object_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    max_attempts: u32,
    finalization_hints: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
    sender: Sender<WitnessVectorArtifacts>,
}

impl WitnessVectorGenerator {
    const POLLING_INTERVAL_MS: u64 = 1500;

    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
        max_attempts: u32,
        finalization_hints: HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
        sender: Sender<WitnessVectorArtifacts>,
    ) -> Self {
        Self {
            object_store,
            connection_pool,
            protocol_version,
            max_attempts,
            finalization_hints,
            sender,
        }
    }

    pub async fn run(self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        let mut start = Instant::now();
        while !cancellation_token.is_cancelled() {
            if let Some((job_id, job)) = self
                .get_next_job()
                .await
                .context("failed during get_next_job()")?
            {
                tracing::info!(
                    "Witness Vector Generator received job after: {:?}",
                    start.elapsed()
                );

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
                                                                                tracing::info!("Witness Vector Generator executed job in: {:?}", started_at.elapsed());
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
                tracing::info!(
                    "Witness Vector Generator executed job in: {:?}",
                    started_at.elapsed()
                );
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

    async fn get_next_job(&self) -> anyhow::Result<Option<(u32, ProverJob)>> {
        let mut connection = self.connection_pool.connection().await.unwrap();
        let pod_name = get_current_pod_name();
        let prover_job_metadata = match connection
            .fri_prover_jobs_dal()
            .get_next_job(self.protocol_version, &pod_name)
            .await
        {
            None => return Ok(None),
            Some(job) => job,
        };
        // tracing::info!("Started processing prover job: {:?}", prover_job_metadata);

        let circuit_key = FriCircuitKey {
            block_number: prover_job_metadata.block_number,
            sequence_number: prover_job_metadata.sequence_number,
            circuit_id: prover_job_metadata.circuit_id,
            aggregation_round: prover_job_metadata.aggregation_round,
            depth: prover_job_metadata.depth,
        };
        // let started_at = Instant::now();
        let circuit_wrapper = self
            .object_store
            .get(circuit_key)
            .await
            .unwrap_or_else(|err| panic!("{err:?}"));
        let input = match circuit_wrapper {
            a @ CircuitWrapper::Base(_) => a,
            a @ CircuitWrapper::Recursive(_) => a,
            CircuitWrapper::BasePartial((circuit, aux_data)) => {
                // inject additional data
                if let ZkSyncBaseLayerCircuit::RAMPermutation(circuit_instance) = circuit {
                    let sorted_witness_key = RamPermutationQueueWitnessKey {
                        block_number: prover_job_metadata.block_number,
                        circuit_subsequence_number: aux_data.circuit_subsequence_number as usize,
                        is_sorted: true,
                    };

                    let sorted_witness_handle = self.object_store.get(sorted_witness_key);

                    let unsorted_witness_key = RamPermutationQueueWitnessKey {
                        block_number: prover_job_metadata.block_number,
                        circuit_subsequence_number: aux_data.circuit_subsequence_number as usize,
                        is_sorted: false,
                    };

                    let unsorted_witness_handle = self.object_store.get(unsorted_witness_key);

                    let unsorted_witness: RamPermutationQueueWitness =
                        unsorted_witness_handle.await.unwrap();
                    let sorted_witness: RamPermutationQueueWitness =
                        sorted_witness_handle.await.unwrap();

                    let mut witness = circuit_instance.witness.take().unwrap();
                    witness.unsorted_queue_witness = FullStateCircuitQueueRawWitness {
                        elements: unsorted_witness.witness.into(),
                    };
                    witness.sorted_queue_witness = FullStateCircuitQueueRawWitness {
                        elements: sorted_witness.witness.into(),
                    };
                    circuit_instance.witness.store(Some(witness));

                    CircuitWrapper::Base(ZkSyncBaseLayerCircuit::RAMPermutation(circuit_instance))
                } else {
                    panic!("Unexpected circuit received with partial witness");
                }
            }
        };

        // let label = CircuitLabels {
        //     circuit_type: prover_job_metadata.circuit_id,
        //     aggregation_round: prover_job_metadata.aggregation_round.into(),
        // };
        // PROVER_FRI_UTILS_METRICS.blob_fetch_time[&label].observe(started_at.elapsed());

        let setup_data_key = ProverServiceDataKey {
            circuit_id: prover_job_metadata.circuit_id,
            round: prover_job_metadata.aggregation_round,
        };
        let prover_job = ProverJob::new(
            prover_job_metadata.block_number,
            prover_job_metadata.id,
            input,
            setup_data_key,
        );
        Ok(Some((prover_job.job_id, prover_job)))
    }

    //     /// Function that processes a job
    async fn process_job(
        &self,
        _job_id: u32,
        job: ProverJob,
        _started_at: Instant,
    ) -> JoinHandle<anyhow::Result<WitnessVectorArtifacts>> {
        let finalization_hints = self
            .finalization_hints
            .get(&job.setup_data_key)
            .expect("no finalization hints for setup_data_key")
            .clone();
        tokio::task::spawn_blocking(move || {
            let block_number = job.block_number;
            let _span = tracing::info_span!("witness_vector_generator", %block_number).entered();
            Self::generate_witness_vector(job, finalization_hints)
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % job.block_number)
    )]
    pub fn generate_witness_vector(
        job: ProverJob,
        finalization_hints: Arc<FinalizationHintsForProver>,
    ) -> anyhow::Result<WitnessVectorArtifacts> {
        let cs = match job.circuit_wrapper.clone() {
            CircuitWrapper::Base(base_circuit) => {
                base_circuit.synthesis::<GoldilocksField>(&finalization_hints)
            }
            CircuitWrapper::Recursive(recursive_circuit) => {
                recursive_circuit.synthesis::<GoldilocksField>(&finalization_hints)
            }
            CircuitWrapper::BasePartial(_) => {
                panic!("Invalid circuit wrapper received for witness vector generation");
            }
        };
        Ok(WitnessVectorArtifacts::new(cs.witness.unwrap(), job))
    }

    async fn save_result(
        &self,
        _job_id: u32,
        _started_at: Instant,
        artifacts: WitnessVectorArtifacts,
    ) {
        let now = Instant::now();
        self.sender.send(artifacts).await.unwrap();
        tracing::info!(
            "Witness Vector Generator sent job after {:?}",
            now.elapsed()
        );
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
}
