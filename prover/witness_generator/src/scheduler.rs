use std::convert::TryInto;
use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::{GoldilocksExt2, GoldilocksField};
use zksync_prover_fri_types::circuit_definitions::boojum::gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::scheduler::SchedulerCircuit;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey,
    ZkSyncRecursiveLayerCircuit, SCHEDULER_CAPACITY,
};
use zksync_prover_fri_types::circuit_definitions::recursion_layer_proof_config;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::input::SchedulerCircuitInstanceWitness;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::SchedulerConfig;
use zksync_vk_setup_data_server_fri::get_recursive_layer_vk_for_circuit_type;
use zksync_vk_setup_data_server_fri::utils::get_leaf_vk_params;

use crate::utils::{load_proofs_for_job_ids, SchedulerPartialInputWrapper};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{FriCircuitKey, ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::{get_current_pod_name, CircuitWrapper, FriProofWrapper};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::AggregationRound;
use zksync_types::protocol_version::FriProtocolVersionId;
use zksync_types::L1BatchNumber;

pub struct SchedulerArtifacts {
    pub scheduler_circuit: ZkSyncRecursiveLayerCircuit,
}

#[derive(Clone)]
pub struct SchedulerWitnessGeneratorJob {
    block_number: L1BatchNumber,
    scheduler_witness: SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
    node_vk: ZkSyncRecursionLayerVerificationKey,
}

#[derive(Debug)]
pub struct SchedulerWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Box<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool,
    protocol_versions: Vec<FriProtocolVersionId>,
}

impl SchedulerWitnessGenerator {
    pub async fn new(
        config: FriWitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        prover_connection_pool: ConnectionPool,
        protocol_versions: Vec<FriProtocolVersionId>,
    ) -> Self {
        Self {
            config,
            object_store: store_factory.create_store().await,
            prover_connection_pool,
            protocol_versions,
        }
    }

    pub fn process_job_sync(
        job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> SchedulerArtifacts {
        tracing::info!(
            "Starting fri witness generation of type {:?} for block {}",
            AggregationRound::Scheduler,
            job.block_number.0
        );
        let config = SchedulerConfig {
            proof_config: recursion_layer_proof_config(),
            vk_fixed_parameters: job.node_vk.into_inner().fixed_parameters,
            capacity: SCHEDULER_CAPACITY,
            _marker: std::marker::PhantomData,
        };

        let scheduler_circuit = SchedulerCircuit {
            witness: job.scheduler_witness,
            config,
            transcript_params: (),
            _marker: std::marker::PhantomData,
        };
        metrics::histogram!(
                    "prover_fri.witness_generation.witness_generation_time",
                    started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::Scheduler),
        );

        tracing::info!(
            "Scheduler generation for block {} is complete in {:?}",
            job.block_number.0,
            started_at.elapsed()
        );

        SchedulerArtifacts {
            scheduler_circuit: ZkSyncRecursiveLayerCircuit::SchedulerCircuit(scheduler_circuit),
        }
    }
}

#[async_trait]
impl JobProcessor for SchedulerWitnessGenerator {
    type Job = SchedulerWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = SchedulerArtifacts;

    const SERVICE_NAME: &'static str = "fri_scheduler_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(l1_batch_number) = prover_connection
            .fri_witness_generator_dal()
            .get_next_scheduler_witness_job(&self.protocol_versions, &pod_name)
            .await
        else {
            return Ok(None);
        };
        let proof_job_ids = prover_connection
            .fri_scheduler_dependency_tracker_dal()
            .get_final_prover_job_ids_for(l1_batch_number)
            .await;

        Ok(Some((
            l1_batch_number,
            prepare_job(l1_batch_number, proof_job_ids, &*self.object_store)
                .await
                .context("prepare_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: L1BatchNumber, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_scheduler_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<SchedulerArtifacts>> {
        tokio::task::spawn_blocking(move || Ok(Self::process_job_sync(job, started_at)))
    }

    async fn save_result(
        &self,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: SchedulerArtifacts,
    ) -> anyhow::Result<()> {
        let key = FriCircuitKey {
            block_number: job_id,
            circuit_id: 1,
            sequence_number: 0,
            depth: 0,
            aggregation_round: AggregationRound::Scheduler,
        };
        let blob_save_started_at = Instant::now();
        let scheduler_circuit_blob_url = self
            .object_store
            .put(key, &CircuitWrapper::Recursive(artifacts.scheduler_circuit))
            .await
            .unwrap();
        metrics::histogram!(
                    "prover_fri.witness_generation.blob_save_time",
                    blob_save_started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::Scheduler),
        );

        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let mut transaction = prover_connection.start_transaction().await.unwrap();
        let protocol_version_id = transaction
            .fri_witness_generator_dal()
            .protocol_version_for_l1_batch(job_id)
            .await;
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                job_id,
                1,
                0,
                0,
                AggregationRound::Scheduler,
                &scheduler_circuit_blob_url,
                false,
                protocol_version_id,
            )
            .await;

        transaction
            .fri_witness_generator_dal()
            .mark_scheduler_job_as_successful(job_id, started_at.elapsed())
            .await;

        transaction.commit().await.unwrap();
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &L1BatchNumber) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .prover_connection_pool
            .access_storage()
            .await
            .context("failed to acquire DB connection for SchedulerWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_scheduler_witness_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for SchedulerWitnessGenerator")
    }
}

pub async fn prepare_job(
    l1_batch_number: L1BatchNumber,
    proof_job_ids: [u32; 13],
    object_store: &dyn ObjectStore,
) -> anyhow::Result<SchedulerWitnessGeneratorJob> {
    let started_at = Instant::now();
    let proofs = load_proofs_for_job_ids(&proof_job_ids, object_store).await;
    metrics::histogram!(
                    "prover_fri.witness_generation.blob_fetch_time",
                    started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::Scheduler),
    );
    let mut recursive_proofs = vec![];
    for wrapper in proofs {
        match wrapper {
            FriProofWrapper::Base(_) => anyhow::bail!(
                "Expected only recursive proofs for scheduler l1 batch {l1_batch_number}"
            ),
            FriProofWrapper::Recursive(recursive_proof) => {
                recursive_proofs.push(recursive_proof.into_inner())
            }
        }
    }

    let started_at = Instant::now();
    let node_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
    )
    .context("get_recursive_layer_vk_for_circuit_type()")?;
    let SchedulerPartialInputWrapper(mut scheduler_witness) =
        object_store.get(l1_batch_number).await.unwrap();
    scheduler_witness.node_layer_vk_witness = node_vk.clone().into_inner();

    scheduler_witness.proof_witnesses = recursive_proofs.into();

    let leaf_vk_commits = get_leaf_vk_params().context("get_leaf_vk_params()")?;
    let leaf_layer_params = leaf_vk_commits
        .iter()
        .map(|el| el.1.clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    scheduler_witness.leaf_layer_parameters = leaf_layer_params;
    metrics::histogram!(
                "prover_fri.witness_generation.prepare_job_time",
                started_at.elapsed(),
                "aggregation_round" => format!("{:?}", AggregationRound::Scheduler),
    );

    Ok(SchedulerWitnessGeneratorJob {
        block_number: l1_batch_number,
        scheduler_witness,
        node_vk,
    })
}
