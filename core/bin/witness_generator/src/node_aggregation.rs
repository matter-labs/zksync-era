use std::collections::HashMap;
use std::env;
use std::time::Instant;

use async_trait::async_trait;

use crate::utils::{save_prover_input_artifacts, track_witness_generation_stage};
use zksync_config::configs::WitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    circuit::{
        LEAF_CIRCUIT_INDEX, LEAF_SPLITTING_FACTOR, NODE_CIRCUIT_INDEX, NODE_SPLITTING_FACTOR,
    },
    proofs::{AggregationRound, PrepareNodeAggregationCircuitJob, WitnessGeneratorJobMetadata},
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::bn256::Bn256,
        bellman::plonk::better_better_cs::setup::VerificationKey,
        ff::to_hex,
        witness::{
            self,
            oracle::VmWitnessOracle,
            recursive_aggregation::{erase_vk_type, padding_aggregations},
        },
        NodeAggregationOutputDataWitness,
    },
    L1BatchNumber,
};
use zksync_verification_key_server::{
    get_vk_for_circuit_type, get_vks_for_basic_circuits, get_vks_for_commitment,
};

pub struct NodeAggregationArtifacts {
    final_node_aggregation: NodeAggregationOutputDataWitness<Bn256>,
    node_circuits: Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

#[derive(Debug)]
struct BlobUrls {
    node_aggregations_url: String,
    circuit_types_and_urls: Vec<(&'static str, String)>,
}

#[derive(Clone)]
pub struct NodeAggregationWitnessGeneratorJob {
    block_number: L1BatchNumber,
    job: PrepareNodeAggregationCircuitJob,
}

#[derive(Debug)]
pub struct NodeAggregationWitnessGenerator {
    config: WitnessGeneratorConfig,
    object_store: Box<dyn ObjectStore>,
}

impl NodeAggregationWitnessGenerator {
    pub fn new(config: WitnessGeneratorConfig, store_factory: &ObjectStoreFactory) -> Self {
        Self {
            config,
            object_store: store_factory.create_store(),
        }
    }

    fn process_job_sync(
        node_job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> NodeAggregationArtifacts {
        let config: WitnessGeneratorConfig = WitnessGeneratorConfig::from_env();
        let NodeAggregationWitnessGeneratorJob { block_number, job } = node_job;

        vlog::info!(
            "Starting witness generation of type {:?} for block {}",
            AggregationRound::NodeAggregation,
            block_number.0
        );
        process_node_aggregation_job(config, started_at, block_number, job)
    }

    fn get_artifacts(
        &self,
        metadata: WitnessGeneratorJobMetadata,
    ) -> NodeAggregationWitnessGeneratorJob {
        let leaf_layer_subqueues = self
            .object_store
            .get(metadata.block_number)
            .expect("leaf_layer_subqueues not found in queued `node_aggregation_witness_jobs` job");
        let aggregation_outputs = self
            .object_store
            .get(metadata.block_number)
            .expect("aggregation_outputs not found in queued `node_aggregation_witness_jobs` job");

        NodeAggregationWitnessGeneratorJob {
            block_number: metadata.block_number,
            job: PrepareNodeAggregationCircuitJob {
                previous_level_proofs: metadata.proofs,
                previous_level_leafs_aggregations: aggregation_outputs,
                previous_sequence: leaf_layer_subqueues,
            },
        }
    }

    fn save_artifacts(
        &self,
        block_number: L1BatchNumber,
        artifacts: NodeAggregationArtifacts,
    ) -> BlobUrls {
        let node_aggregations_url = self
            .object_store
            .put(block_number, &artifacts.final_node_aggregation)
            .unwrap();
        let circuit_types_and_urls = save_prover_input_artifacts(
            block_number,
            &artifacts.node_circuits,
            &*self.object_store,
            AggregationRound::NodeAggregation,
        );
        BlobUrls {
            node_aggregations_url,
            circuit_types_and_urls,
        }
    }
}

#[async_trait]
impl JobProcessor for NodeAggregationWitnessGenerator {
    type Job = NodeAggregationWitnessGeneratorJob;
    type JobId = L1BatchNumber;
    type JobArtifacts = NodeAggregationArtifacts;

    const SERVICE_NAME: &'static str = "node_aggregation_witness_generator";

    async fn get_next_job(
        &self,
        connection_pool: ConnectionPool,
    ) -> Option<(Self::JobId, Self::Job)> {
        let mut connection = connection_pool.access_storage_blocking();
        let last_l1_batch_to_process = self.config.last_l1_batch_to_process();

        match connection
            .witness_generator_dal()
            .get_next_node_aggregation_witness_job(
                self.config.witness_generation_timeout(),
                self.config.max_attempts,
                last_l1_batch_to_process,
            ) {
            Some(metadata) => {
                let job = self.get_artifacts(metadata);
                return Some((job.block_number, job));
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
                AggregationRound::NodeAggregation,
                started_at.elapsed(),
                error,
                self.config.max_attempts,
            );
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _connection_pool: ConnectionPool,
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<NodeAggregationArtifacts> {
        tokio::task::spawn_blocking(move || Self::process_job_sync(job, started_at))
    }

    async fn save_result(
        &self,
        connection_pool: ConnectionPool,
        job_id: L1BatchNumber,
        started_at: Instant,
        artifacts: NodeAggregationArtifacts,
    ) {
        let blob_urls = self.save_artifacts(job_id, artifacts);
        update_database(connection_pool, started_at, job_id, blob_urls);
    }
}

pub fn process_node_aggregation_job(
    config: WitnessGeneratorConfig,
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareNodeAggregationCircuitJob,
) -> NodeAggregationArtifacts {
    let stage_started_at = Instant::now();
    zksync_prover_utils::ensure_initial_setup_keys_present(
        &config.initial_setup_key_path,
        &config.key_download_url,
    );
    env::set_var("CRS_FILE", config.initial_setup_key_path);
    vlog::info!("Keys loaded in {:?}", stage_started_at.elapsed());
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    let padding_aggregations = padding_aggregations(NODE_SPLITTING_FACTOR);

    let (_, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    let node_aggregation_vk = get_vk_for_circuit_type(NODE_CIRCUIT_INDEX);

    let leaf_aggregation_vk = get_vk_for_circuit_type(LEAF_CIRCUIT_INDEX);

    let (_, leaf_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            leaf_aggregation_vk.clone(),
        ));

    let (_, node_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            node_aggregation_vk,
        ));

    vlog::info!(
        "commitments: basic set: {:?}, leaf: {:?}, node: {:?}",
        to_hex(&set_committment),
        to_hex(&leaf_aggregation_vk_committment),
        to_hex(&node_aggregation_vk_committment)
    );
    vlog::info!("Commitments generated in {:?}", stage_started_at.elapsed());

    // fs::write("previous_level_proofs.bincode", bincode::serialize(&job.previous_level_proofs).unwrap()).unwrap();
    // fs::write("leaf_aggregation_vk.bincode", bincode::serialize(&leaf_aggregation_vk).unwrap()).unwrap();
    // fs::write("previous_level_leafs_aggregations.bincode", bincode::serialize(&job.previous_level_leafs_aggregations).unwrap()).unwrap();
    // fs::write("previous_sequence.bincode", bincode::serialize(&job.previous_sequence).unwrap()).unwrap();
    // fs::write("padding_aggregations.bincode", bincode::serialize(&padding_aggregations).unwrap()).unwrap();
    // fs::write("set_committment.bincode", bincode::serialize(&set_committment).unwrap()).unwrap();
    // fs::write("node_aggregation_vk_committment.bincode", bincode::serialize(&node_aggregation_vk_committment).unwrap()).unwrap();
    // fs::write("leaf_aggregation_vk_committment.bincode", bincode::serialize(&leaf_aggregation_vk_committment).unwrap()).unwrap();
    // fs::write("g2_points.bincode", bincode::serialize(&g2_points).unwrap()).unwrap();

    let stage_started_at = Instant::now();
    let (_, final_node_aggregations, node_circuits) =
        zksync_types::zkevm_test_harness::witness::recursive_aggregation::prepare_node_aggregations(
            job.previous_level_proofs,
            leaf_aggregation_vk,
            true,
            0,
            job.previous_level_leafs_aggregations,
            Vec::default(),
            job.previous_sequence,
            LEAF_SPLITTING_FACTOR,
            NODE_SPLITTING_FACTOR,
            padding_aggregations,
            set_committment,
            node_aggregation_vk_committment,
            leaf_aggregation_vk_committment,
            g2_points,
        );

    vlog::info!(
        "prepare_node_aggregations took {:?}",
        stage_started_at.elapsed()
    );

    assert_eq!(
        node_circuits.len(),
        1,
        "prepare_node_aggregations returned more than one circuit"
    );
    assert_eq!(
        final_node_aggregations.len(),
        1,
        "prepare_node_aggregations returned more than one node aggregation"
    );

    vlog::info!(
        "Node witness generation for block {} is complete in {:?}. Number of circuits: {}",
        block_number.0,
        started_at.elapsed(),
        node_circuits.len()
    );

    NodeAggregationArtifacts {
        final_node_aggregation: final_node_aggregations.into_iter().next().unwrap(),
        node_circuits,
    }
}

fn update_database(
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    blob_urls: BlobUrls,
) {
    let mut connection = connection_pool.access_storage_blocking();
    let mut transaction = connection.start_transaction_blocking();

    // inserts artifacts into the scheduler_witness_jobs table
    // and advances it to waiting_for_proofs status
    transaction
        .witness_generator_dal()
        .save_node_aggregation_artifacts(block_number, &blob_urls.node_aggregations_url);
    transaction.prover_dal().insert_prover_jobs(
        block_number,
        blob_urls.circuit_types_and_urls,
        AggregationRound::NodeAggregation,
    );
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::NodeAggregation,
            started_at.elapsed(),
        );

    transaction.commit_blocking();
    track_witness_generation_stage(started_at, AggregationRound::NodeAggregation);
}
