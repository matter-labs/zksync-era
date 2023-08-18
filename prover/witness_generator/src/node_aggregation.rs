use std::time::Instant;

use async_trait::async_trait;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::{
    ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
    ZkSyncRecursionLayerVerificationKey, ZkSyncRecursiveLayerCircuit,
};
use zksync_prover_fri_types::circuit_definitions::encodings::recursion_request::RecursionQueueSimulator;

use zkevm_test_harness::witness::recursive_aggregation::{
    compute_node_vk_commitment, create_node_witnesses,
};
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness;
use zksync_vk_setup_data_server_fri::get_recursive_layer_vk_for_circuit_type;
use zksync_vk_setup_data_server_fri::utils::get_leaf_vk_params;

use crate::utils::{
    load_proofs_for_job_ids, save_node_aggregations_artifacts,
    save_recursive_layer_prover_input_artifacts, AggregationWrapper,
};
use zksync_dal::ConnectionPool;
use zksync_object_store::{AggregationsKey, ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::FriProofWrapper;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::NodeAggregationJobMetadata;
use zksync_types::{proofs::AggregationRound, L1BatchNumber};

pub struct NodeAggregationArtifacts {
    circuit_id: u8,
    block_number: L1BatchNumber,
    depth: u16,
    pub next_aggregations: Vec<(
        u64,
        RecursionQueueSimulator<GoldilocksField>,
        ZkSyncRecursiveLayerCircuit,
    )>,
}

#[derive(Debug)]
struct BlobUrls {
    node_aggregations_url: String,
    circuit_ids_and_urls: Vec<(u8, String)>,
}

#[derive(Clone)]
pub struct NodeAggregationWitnessGeneratorJob {
    circuit_id: u8,
    block_number: L1BatchNumber,
    depth: u16,
    aggregations: Vec<(
        u64,
        RecursionQueueSimulator<GoldilocksField>,
        ZkSyncRecursiveLayerCircuit,
    )>,
    proofs: Vec<ZkSyncRecursionLayerProof>,
    leaf_vk: ZkSyncRecursionLayerVerificationKey,
    node_vk: ZkSyncRecursionLayerVerificationKey,
    all_leafs_layer_params: Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>,
}

#[derive(Debug)]
pub struct NodeAggregationWitnessGenerator {
    object_store: Box<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool,
}

impl NodeAggregationWitnessGenerator {
    pub async fn new(
        store_factory: &ObjectStoreFactory,
        prover_connection_pool: ConnectionPool,
    ) -> Self {
        Self {
            object_store: store_factory.create_store().await,
            prover_connection_pool,
        }
    }

    pub fn process_job_sync(
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> NodeAggregationArtifacts {
        let node_vk_commitment = compute_node_vk_commitment(job.node_vk.clone());
        vlog::info!(
            "Starting witness generation of type {:?} for block {} circuit id {} depth {}",
            AggregationRound::NodeAggregation,
            job.block_number.0,
            job.circuit_id,
            job.depth
        );
        let vk = match job.depth {
            0 => job.leaf_vk,
            _ => job.node_vk,
        };
        let next_aggregations = create_node_witnesses(
            job.aggregations,
            job.proofs,
            vk,
            node_vk_commitment,
            &job.all_leafs_layer_params,
        );
        metrics::histogram!(
                    "prover_fri.witness_generation.witness_generation_time",
                    started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::NodeAggregation),
        );
        vlog::info!(
        "Node witness generation for block {} with circuit id {} at depth {} with {} next_aggregations jobs completed in {:?}.",
        job.block_number.0,
        job.circuit_id,
        job.depth,
        next_aggregations.len(),
        started_at.elapsed(),
    );

        NodeAggregationArtifacts {
            circuit_id: job.circuit_id,
            block_number: job.block_number,
            depth: job.depth + 1,
            next_aggregations,
        }
    }
}

#[async_trait]
impl JobProcessor for NodeAggregationWitnessGenerator {
    type Job = NodeAggregationWitnessGeneratorJob;
    type JobId = u32;
    type JobArtifacts = NodeAggregationArtifacts;

    const SERVICE_NAME: &'static str = "fri_node_aggregation_witness_generator";

    async fn get_next_job(&self) -> Option<(Self::JobId, Self::Job)> {
        let mut prover_connection = self.prover_connection_pool.access_storage().await;
        let metadata = prover_connection
            .fri_witness_generator_dal()
            .get_next_node_aggregation_job()
            .await?;
        vlog::info!("Processing node aggregation job {:?}", metadata.id);
        Some((
            metadata.id,
            prepare_job(metadata, &*self.object_store).await,
        ))
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .access_storage()
            .await
            .fri_witness_generator_dal()
            .mark_node_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<NodeAggregationArtifacts> {
        tokio::task::spawn_blocking(move || Self::process_job_sync(job, started_at))
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: NodeAggregationArtifacts,
    ) {
        let block_number = artifacts.block_number;
        let circuit_id = artifacts.circuit_id;
        let depth = artifacts.depth;
        let shall_continue_node_aggregations = artifacts.next_aggregations.len() > 1;
        let blob_urls = save_artifacts(artifacts, &*self.object_store).await;
        update_database(
            &self.prover_connection_pool,
            started_at,
            job_id,
            block_number,
            depth,
            circuit_id,
            blob_urls,
            shall_continue_node_aggregations,
        )
        .await;
    }
}

pub async fn prepare_job(
    metadata: NodeAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> NodeAggregationWitnessGeneratorJob {
    let started_at = Instant::now();
    let artifacts = get_artifacts(&metadata, object_store).await;
    let proofs = load_proofs_for_job_ids(&metadata.prover_job_ids_for_proofs, object_store).await;
    metrics::histogram!(
                    "prover_fri.witness_generation.blob_fetch_time",
                    started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::NodeAggregation),
    );
    let started_at = Instant::now();
    let leaf_vk = get_recursive_layer_vk_for_circuit_type(metadata.circuit_id);
    let node_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
    );

    let recursive_proofs = proofs
        .into_iter()
        .map(|wrapper| match wrapper {
            FriProofWrapper::Base(_) => {
                panic!(
                    "Expected only recursive proofs for node agg {}",
                    metadata.id
                )
            }
            FriProofWrapper::Recursive(recursive_proof) => recursive_proof,
        })
        .collect::<Vec<_>>();

    metrics::histogram!(
                    "prover_fri.witness_generation.job_preparation_time",
                    started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::NodeAggregation),
    );
    NodeAggregationWitnessGeneratorJob {
        circuit_id: metadata.circuit_id,
        block_number: metadata.block_number,
        depth: metadata.depth,
        aggregations: artifacts.0,
        proofs: recursive_proofs,
        leaf_vk,
        node_vk,
        all_leafs_layer_params: get_leaf_vk_params(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn update_database(
    prover_connection_pool: &ConnectionPool,
    started_at: Instant,
    id: u32,
    block_number: L1BatchNumber,
    depth: u16,
    circuit_id: u8,
    blob_urls: BlobUrls,
    shall_continue_node_aggregations: bool,
) {
    let mut prover_connection = prover_connection_pool.access_storage().await;
    let mut transaction = prover_connection.start_transaction().await;
    let dependent_jobs = blob_urls.circuit_ids_and_urls.len();
    match shall_continue_node_aggregations {
        true => {
            transaction
                .fri_prover_jobs_dal()
                .insert_prover_jobs(
                    block_number,
                    blob_urls.circuit_ids_and_urls,
                    AggregationRound::NodeAggregation,
                    depth,
                )
                .await;
            transaction
                .fri_witness_generator_dal()
                .insert_node_aggregation_jobs(
                    block_number,
                    circuit_id,
                    Some(dependent_jobs as i32),
                    depth,
                    &blob_urls.node_aggregations_url,
                )
                .await;
        }
        false => {
            let (_, blob_url) = blob_urls.circuit_ids_and_urls[0].clone();
            transaction
                .fri_prover_jobs_dal()
                .insert_prover_job(
                    block_number,
                    circuit_id,
                    depth,
                    0,
                    AggregationRound::NodeAggregation,
                    &blob_url,
                    true,
                )
                .await
        }
    }

    transaction
        .fri_witness_generator_dal()
        .mark_node_aggregation_as_successful(id, started_at.elapsed())
        .await;

    transaction.commit().await;
}

async fn get_artifacts(
    metadata: &NodeAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> AggregationWrapper {
    let key = AggregationsKey {
        block_number: metadata.block_number,
        circuit_id: metadata.circuit_id,
        depth: metadata.depth,
    };
    object_store
        .get(key)
        .await
        .unwrap_or_else(|_| panic!("node aggregation job artifacts missing: {:?}", key))
}

async fn save_artifacts(
    artifacts: NodeAggregationArtifacts,
    object_store: &dyn ObjectStore,
) -> BlobUrls {
    let started_at = Instant::now();
    let aggregations_urls = save_node_aggregations_artifacts(
        artifacts.block_number,
        artifacts.circuit_id,
        artifacts.depth,
        artifacts.next_aggregations.clone(),
        object_store,
    )
    .await;
    let circuit_ids_and_urls = save_recursive_layer_prover_input_artifacts(
        artifacts.block_number,
        artifacts.next_aggregations,
        AggregationRound::NodeAggregation,
        artifacts.depth,
        object_store,
        Some(artifacts.circuit_id),
    )
    .await;
    metrics::histogram!(
                    "prover_fri.witness_generation.blob_save_time",
                    started_at.elapsed(),
                    "aggregation_round" => format!("{:?}", AggregationRound::NodeAggregation),
    );
    BlobUrls {
        node_aggregations_url: aggregations_urls,
        circuit_ids_and_urls,
    }
}
