use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;

use crate::utils::{save_prover_input_artifacts, track_witness_generation_stage};
use zksync_config::configs::WitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    circuit::LEAF_SPLITTING_FACTOR,
    proofs::{AggregationRound, PrepareLeafAggregationCircuitsJob, WitnessGeneratorJobMetadata},
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit, bellman::bn256::Bn256,
        bellman::plonk::better_better_cs::setup::VerificationKey,
        encodings::recursion_request::RecursionRequest, encodings::QueueSimulator, witness,
        witness::oracle::VmWitnessOracle, LeafAggregationOutputDataWitness,
    },
    L1BatchNumber,
};
use zksync_verification_key_server::{
    get_ordered_vks_for_basic_circuits, get_vks_for_basic_circuits, get_vks_for_commitment,
};

pub struct LeafAggregationArtifacts {
    leaf_layer_subqueues: Vec<QueueSimulator<Bn256, RecursionRequest<Bn256>, 2, 2>>,
    aggregation_outputs: Vec<LeafAggregationOutputDataWitness<Bn256>>,
    leaf_circuits: Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

#[derive(Debug)]
struct BlobUrls {
    leaf_layer_subqueues_url: String,
    aggregation_outputs_url: String,
    circuit_types_and_urls: Vec<(&'static str, String)>,
}

#[derive(Clone)]
pub struct LeafAggregationWitnessGeneratorJob {
    block_number: L1BatchNumber,
    job: PrepareLeafAggregationCircuitsJob,
}

#[derive(Debug)]
pub struct LeafAggregationWitnessGenerator {
    config: WitnessGeneratorConfig,
    object_store: Box<dyn ObjectStore>,
}

impl LeafAggregationWitnessGenerator {
    pub fn new(config: WitnessGeneratorConfig, store_factory: &ObjectStoreFactory) -> Self {
        Self {
            config,
            object_store: store_factory.create_store(),
        }
    }

    fn process_job_sync(
        leaf_job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> LeafAggregationArtifacts {
        let LeafAggregationWitnessGeneratorJob { block_number, job } = leaf_job;

        vlog::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::LeafAggregation,
            block_number.0
        );
        process_leaf_aggregation_job(started_at, block_number, job)
    }

    fn get_artifacts(
        &self,
        metadata: WitnessGeneratorJobMetadata,
    ) -> LeafAggregationWitnessGeneratorJob {
        let basic_circuits = self.object_store.get(metadata.block_number).unwrap();
        let basic_circuits_inputs = self.object_store.get(metadata.block_number).unwrap();

        LeafAggregationWitnessGeneratorJob {
            block_number: metadata.block_number,
            job: PrepareLeafAggregationCircuitsJob {
                basic_circuits_inputs,
                basic_circuits_proofs: metadata.proofs,
                basic_circuits,
            },
        }
    }

    fn save_artifacts(
        &self,
        block_number: L1BatchNumber,
        artifacts: LeafAggregationArtifacts,
    ) -> BlobUrls {
        let leaf_layer_subqueues_url = self
            .object_store
            .put(block_number, &artifacts.leaf_layer_subqueues)
            .unwrap();
        let aggregation_outputs_url = self
            .object_store
            .put(block_number, &artifacts.aggregation_outputs)
            .unwrap();
        let circuit_types_and_urls = save_prover_input_artifacts(
            block_number,
            &artifacts.leaf_circuits,
            &*self.object_store,
            AggregationRound::LeafAggregation,
        );
        BlobUrls {
            leaf_layer_subqueues_url,
            aggregation_outputs_url,
            circuit_types_and_urls,
        }
    }
}

#[async_trait]
impl JobProcessor for LeafAggregationWitnessGenerator {
    type Job = LeafAggregationWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = LeafAggregationArtifacts;

    const SERVICE_NAME: &'static str = "leaf_aggregation_witness_generator";

    async fn get_next_job(
        &self,
        connection_pool: ConnectionPool,
    ) -> Option<(Self::JobId, Self::Job)> {
        let mut connection = connection_pool.access_storage_blocking();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();

        match connection
            .witness_generator_dal()
            .get_next_leaf_aggregation_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
            ) {
            Some(metadata) => {
                let job = self.get_artifacts(metadata);
                Some((job.block_number, job))
            }
            None => None,
        }
    }

    async fn save_failure(
        &self,
        connection_pool: ConnectionPool,
        job_id: L1BatchNumber,
        started_at: Instant,
        error: String,
    ) {
        connection_pool
            .access_storage_blocking()
            .witness_generator_dal()
            .mark_witness_job_as_failed(
                job_id,
                AggregationRound::LeafAggregation,
                started_at.elapsed(),
                error,
                self.config.max_attempts,
            );
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _connection_pool: ConnectionPool,
        job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<LeafAggregationArtifacts> {
        tokio::task::spawn_blocking(move || Self::process_job_sync(job, started_at))
    }

    async fn save_result(
        &self,
        connection_pool: ConnectionPool,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: LeafAggregationArtifacts,
    ) {
        let leaf_circuits_len = artifacts.leaf_circuits.len();
        let blob_urls = self.save_artifacts(job_id, artifacts);
        update_database(
            connection_pool,
            started_at,
            job_id,
            leaf_circuits_len,
            blob_urls,
        );
    }
}

fn process_leaf_aggregation_job(
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareLeafAggregationCircuitsJob,
) -> LeafAggregationArtifacts {
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    vlog::info!(
        "Verification keys loaded in {:?}",
        stage_started_at.elapsed()
    );

    // we need the list of vks that matches the list of job.basic_circuit_proofs
    let vks_for_aggregation: Vec<
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_ordered_vks_for_basic_circuits(&job.basic_circuits, &verification_keys);

    let (all_vk_committments, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    vlog::info!("Commitments generated in {:?}", stage_started_at.elapsed());

    let stage_started_at = Instant::now();

    let (leaf_layer_subqueues, aggregation_outputs, leaf_circuits) =
        witness::recursive_aggregation::prepare_leaf_aggregations(
            job.basic_circuits,
            job.basic_circuits_inputs,
            job.basic_circuits_proofs,
            vks_for_aggregation,
            LEAF_SPLITTING_FACTOR,
            all_vk_committments,
            set_committment,
            g2_points,
        );

    vlog::info!(
        "prepare_leaf_aggregations took {:?}",
        stage_started_at.elapsed()
    );
    vlog::info!(
        "Leaf witness generation for block {} is complete in {:?}. Number of circuits: {}",
        block_number.0,
        started_at.elapsed(),
        leaf_circuits.len()
    );

    LeafAggregationArtifacts {
        leaf_layer_subqueues,
        aggregation_outputs,
        leaf_circuits,
    }
}

fn update_database(
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    leaf_circuits_len: usize,
    blob_urls: BlobUrls,
) {
    let mut connection = connection_pool.access_storage_blocking();
    let mut transaction = connection.start_transaction_blocking();

    // inserts artifacts into the node_aggregation_witness_jobs table
    // and advances it to waiting_for_proofs status
    transaction
        .witness_generator_dal()
        .save_leaf_aggregation_artifacts(
            block_number,
            leaf_circuits_len,
            &blob_urls.leaf_layer_subqueues_url,
            &blob_urls.aggregation_outputs_url,
        );
    transaction.prover_dal().insert_prover_jobs(
        block_number,
        blob_urls.circuit_types_and_urls,
        AggregationRound::LeafAggregation,
    );
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::LeafAggregation,
            started_at.elapsed(),
        );

    transaction.commit_blocking();
    track_witness_generation_stage(started_at, AggregationRound::LeafAggregation);
}
