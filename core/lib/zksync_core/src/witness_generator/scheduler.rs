use async_trait::async_trait;

use std::{collections::HashMap, slice, time::Instant};

use zksync_config::configs::WitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    circuit::{
        LEAF_CIRCUIT_INDEX, LEAF_SPLITTING_FACTOR, NODE_CIRCUIT_INDEX, NODE_SPLITTING_FACTOR,
    },
    proofs::{AggregationRound, PrepareSchedulerCircuitJob, WitnessGeneratorJobMetadata},
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::{bn256::Bn256, plonk::better_better_cs::setup::VerificationKey},
        sync_vm::scheduler::BlockApplicationWitness,
        witness::{self, oracle::VmWitnessOracle, recursive_aggregation::erase_vk_type},
    },
    L1BatchNumber, ProtocolVersionId,
};
use zksync_verification_key_server::{
    get_vk_for_circuit_type, get_vks_for_basic_circuits, get_vks_for_commitment,
};

use super::{utils::save_prover_input_artifacts, METRICS};

pub struct SchedulerArtifacts {
    final_aggregation_result: BlockApplicationWitness<Bn256>,
    scheduler_circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>,
}

#[derive(Clone)]
pub struct SchedulerWitnessGeneratorJob {
    block_number: L1BatchNumber,
    job: PrepareSchedulerCircuitJob,
}

#[derive(Debug)]
pub struct SchedulerWitnessGenerator {
    config: WitnessGeneratorConfig,
    object_store: Box<dyn ObjectStore>,
    protocol_versions: Vec<ProtocolVersionId>,
    connection_pool: ConnectionPool,
    prover_connection_pool: ConnectionPool,
}

impl SchedulerWitnessGenerator {
    pub async fn new(
        config: WitnessGeneratorConfig,
        store_factory: &ObjectStoreFactory,
        protocol_versions: Vec<ProtocolVersionId>,
        connection_pool: ConnectionPool,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            config,
            object_store: store_factory.create_store().await,
            protocol_versions,
            connection_pool,
            prover_connection_pool,
        }
    }

    fn process_job_sync(
        scheduler_job: SchedulerWitnessGeneratorJob,
        started_at: Instant,
    ) -> SchedulerArtifacts {
        let SchedulerWitnessGeneratorJob { block_number, job } = scheduler_job;

        tracing::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::Scheduler,
            block_number.0
        );
        process_scheduler_job(started_at, block_number, job)
    }
}

#[async_trait]
impl JobProcessor for SchedulerWitnessGenerator {
    type Job = SchedulerWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = SchedulerArtifacts;

    const SERVICE_NAME: &'static str = "scheduler_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut connection = self.connection_pool.access_storage().await.unwrap();
        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();

        match prover_connection
            .witness_generator_dal()
            .get_next_scheduler_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
                &self.protocol_versions,
            )
            .await
        {
            Some(metadata) => {
                let prev_metadata = connection
                    .blocks_dal()
                    .get_l1_batch_metadata(metadata.block_number - 1)
                    .await
                    .unwrap();
                let previous_aux_hash = prev_metadata
                    .as_ref()
                    .map_or([0u8; 32], |e| e.metadata.aux_data_hash.0);
                let previous_meta_hash =
                    prev_metadata.map_or([0u8; 32], |e| e.metadata.meta_parameters_hash.0);
                let job = get_artifacts(
                    metadata,
                    previous_aux_hash,
                    previous_meta_hash,
                    &*self.object_store,
                )
                .await;
                Ok(Some((job.block_number, job)))
            }
            None => Ok(None),
        }
    }

    async fn save_failure(&self, job_id: L1BatchNumber, started_at: Instant, error: String) -> () {
        let attempts = self
            .prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .witness_generator_dal()
            .mark_witness_job_as_failed(
                AggregationRound::Scheduler,
                job_id,
                started_at.elapsed(),
                error,
            )
            .await;

        if attempts >= self.config.max_attempts {
            self.connection_pool
                .access_storage()
                .await
                .unwrap()
                .blocks_dal()
                .set_skip_proof_for_l1_batch(job_id)
                .await
                .unwrap();
        }
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
        let circuit_types_and_urls =
            save_artifacts(job_id, &artifacts.scheduler_circuit, &*self.object_store).await;
        update_database(
            &self.connection_pool,
            &self.prover_connection_pool,
            started_at,
            job_id,
            artifacts.final_aggregation_result,
            circuit_types_and_urls,
        )
        .await;
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, _job_id: &Self::JobId) -> anyhow::Result<u32> {
        // Witness generator will be removed soon in favor of FRI one, so returning blank value.
        Ok(1)
    }
}

pub fn process_scheduler_job(
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareSchedulerCircuitJob,
) -> SchedulerArtifacts {
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    let (_, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    tracing::info!(
        "Verification keys loaded in {:?}",
        stage_started_at.elapsed()
    );

    let leaf_aggregation_vk = get_vk_for_circuit_type(LEAF_CIRCUIT_INDEX);

    let node_aggregation_vk = get_vk_for_circuit_type(NODE_CIRCUIT_INDEX);

    let (_, leaf_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            leaf_aggregation_vk,
        ));

    let (_, node_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            node_aggregation_vk.clone(),
        ));

    tracing::info!("Commitments generated in {:?}", stage_started_at.elapsed());
    let stage_started_at = Instant::now();

    let (scheduler_circuit, final_aggregation_result) =
        witness::recursive_aggregation::prepare_scheduler_circuit(
            job.incomplete_scheduler_witness,
            job.node_final_proof_level_proof,
            node_aggregation_vk,
            job.final_node_aggregations,
            set_committment,
            node_aggregation_vk_committment,
            leaf_aggregation_vk_committment,
            job.previous_aux_hash,
            job.previous_meta_hash,
            (LEAF_SPLITTING_FACTOR * NODE_SPLITTING_FACTOR) as u32,
            g2_points,
        );

    tracing::info!(
        "prepare_scheduler_circuit took {:?}",
        stage_started_at.elapsed()
    );

    tracing::info!(
        "Scheduler generation for block {} is complete in {:?}",
        block_number.0,
        started_at.elapsed()
    );

    SchedulerArtifacts {
        final_aggregation_result,
        scheduler_circuit,
    }
}

pub async fn update_database(
    connection_pool: &ConnectionPool,
    prover_connection_pool: &ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    final_aggregation_result: BlockApplicationWitness<Bn256>,
    circuit_types_and_urls: Vec<(&'static str, String)>,
) {
    let mut connection = connection_pool.access_storage().await.unwrap();
    let block = connection
        .blocks_dal()
        .get_l1_batch_metadata(block_number)
        .await
        .unwrap()
        .expect("L1 batch should exist");

    assert_eq!(
        block.metadata.aux_data_hash.0, final_aggregation_result.aux_data_hash,
        "Commitment for aux data is wrong"
    );

    assert_eq!(
        block.metadata.pass_through_data_hash.0, final_aggregation_result.passthrough_data_hash,
        "Commitment for pass through data is wrong"
    );

    assert_eq!(
        block.metadata.meta_parameters_hash.0, final_aggregation_result.meta_data_hash,
        "Commitment for metadata is wrong"
    );

    assert_eq!(
        block.metadata.commitment.0, final_aggregation_result.block_header_hash,
        "Commitment is wrong"
    );

    let mut prover_connection = prover_connection_pool.access_storage().await.unwrap();
    let mut transaction = prover_connection.start_transaction().await.unwrap();
    let protocol_version = transaction
        .witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await
        .unwrap_or_else(|| {
            panic!(
                "No system version exist for l1 batch {} for node agg",
                block_number.0
            )
        });
    transaction
        .prover_dal()
        .insert_prover_jobs(
            block_number,
            circuit_types_and_urls,
            AggregationRound::Scheduler,
            protocol_version,
        )
        .await;

    transaction
        .witness_generator_dal()
        .save_final_aggregation_result(
            block_number,
            final_aggregation_result.aggregation_result_coords,
        )
        .await;

    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::Scheduler,
            started_at.elapsed(),
        )
        .await;

    transaction.commit().await.unwrap();
    METRICS.processing_time[&AggregationRound::Scheduler.into()].observe(started_at.elapsed());
}

async fn save_artifacts(
    block_number: L1BatchNumber,
    scheduler_circuit: &ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>,
    object_store: &dyn ObjectStore,
) -> Vec<(&'static str, String)> {
    save_prover_input_artifacts(
        block_number,
        slice::from_ref(scheduler_circuit),
        object_store,
        AggregationRound::Scheduler,
    )
    .await
}

async fn get_artifacts(
    metadata: WitnessGeneratorJobMetadata,
    previous_aux_hash: [u8; 32],
    previous_meta_hash: [u8; 32],
    object_store: &dyn ObjectStore,
) -> SchedulerWitnessGeneratorJob {
    let scheduler_witness = object_store.get(metadata.block_number).await.unwrap();
    let final_node_aggregations = object_store.get(metadata.block_number).await.unwrap();

    SchedulerWitnessGeneratorJob {
        block_number: metadata.block_number,
        job: PrepareSchedulerCircuitJob {
            incomplete_scheduler_witness: scheduler_witness,
            final_node_aggregations,
            node_final_proof_level_proof: metadata.proofs.into_iter().next().unwrap(),
            previous_aux_hash,
            previous_meta_hash,
        },
    }
}
