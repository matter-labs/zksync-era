use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use zkevm_test_harness::witness::recursive_aggregation::{
    compute_node_vk_commitment, create_node_witnesses,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::recursion_layer::{
            ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
            ZkSyncRecursionLayerVerificationKey, ZkSyncRecursiveLayerCircuit,
        },
        encodings::recursion_request::RecursionQueueSimulator,
        zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
    },
    get_current_pod_name,
    keys::AggregationsKey,
    FriProofWrapper,
};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::NodeAggregationJobMetadata, L1BatchNumber,
};
use zksync_vk_setup_data_server_fri::{keystore::Keystore, utils::get_leaf_vk_params};

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    utils::{
        load_proofs_for_job_ids, save_node_aggregations_artifacts,
        save_recursive_layer_prover_input_artifacts, AggregationWrapper,
    },
};

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
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
}

impl NodeAggregationWitnessGenerator {
    pub fn new(
        config: FriWitnessGeneratorConfig,
        object_store: Arc<dyn ObjectStore>,
        prover_connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        Self {
            config,
            object_store,
            prover_connection_pool,
            protocol_version,
        }
    }

    pub fn process_job_sync(
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> NodeAggregationArtifacts {
        let node_vk_commitment = compute_node_vk_commitment(job.node_vk.clone());
        tracing::info!(
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
        WITNESS_GENERATOR_METRICS.witness_generation_time
            [&AggregationRound::NodeAggregation.into()]
            .observe(started_at.elapsed());

        tracing::info!(
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

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some(metadata) = prover_connection
            .fri_witness_generator_dal()
            .get_next_node_aggregation_job(self.protocol_version, &pod_name)
            .await
        else {
            return Ok(None);
        };
        tracing::info!("Processing node aggregation job {:?}", metadata.id);
        Ok(Some((
            metadata.id,
            prepare_job(metadata, &*self.object_store)
                .await
                .context("prepare_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_node_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<NodeAggregationArtifacts>> {
        tokio::task::spawn_blocking(move || {
            let block_number = job.block_number;
            let _span = tracing::info_span!("node_aggregation", %block_number).entered();
            Ok(Self::process_job_sync(job, started_at))
        })
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: NodeAggregationArtifacts,
    ) -> anyhow::Result<()> {
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
        Ok(())
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }

    async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
        let mut prover_storage = self
            .prover_connection_pool
            .connection()
            .await
            .context("failed to acquire DB connection for NodeAggregationWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_node_aggregation_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for NodeAggregationWitnessGenerator")
    }
}

pub async fn prepare_job(
    metadata: NodeAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> anyhow::Result<NodeAggregationWitnessGeneratorJob> {
    let started_at = Instant::now();
    let artifacts = get_artifacts(&metadata, object_store).await;
    let proofs = load_proofs_for_job_ids(&metadata.prover_job_ids_for_proofs, object_store).await;

    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::NodeAggregation.into()]
        .observe(started_at.elapsed());

    let started_at = Instant::now();
    let keystore = Keystore::default();
    let leaf_vk = keystore
        .load_recursive_layer_verification_key(metadata.circuit_id)
        .context("get_recursive_layer_vk_for_circuit_type")?;
    let node_vk = keystore
        .load_recursive_layer_verification_key(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
        )
        .context("get_recursive_layer_vk_for_circuit_type()")?;

    let mut recursive_proofs = vec![];
    for wrapper in proofs {
        match wrapper {
            FriProofWrapper::Base(_) => {
                anyhow::bail!(
                    "Expected only recursive proofs for node agg {}",
                    metadata.id
                );
            }
            FriProofWrapper::Recursive(recursive_proof) => recursive_proofs.push(recursive_proof),
        }
    }

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::NodeAggregation.into()]
        .observe(started_at.elapsed());

    Ok(NodeAggregationWitnessGeneratorJob {
        circuit_id: metadata.circuit_id,
        block_number: metadata.block_number,
        depth: metadata.depth,
        aggregations: artifacts.0,
        proofs: recursive_proofs,
        leaf_vk,
        node_vk,
        all_leafs_layer_params: get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn update_database(
    prover_connection_pool: &ConnectionPool<Prover>,
    started_at: Instant,
    id: u32,
    block_number: L1BatchNumber,
    depth: u16,
    circuit_id: u8,
    blob_urls: BlobUrls,
    shall_continue_node_aggregations: bool,
) {
    let mut prover_connection = prover_connection_pool.connection().await.unwrap();
    let mut transaction = prover_connection.start_transaction().await.unwrap();
    let dependent_jobs = blob_urls.circuit_ids_and_urls.len();
    let protocol_version_id = transaction
        .fri_witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await;
    match shall_continue_node_aggregations {
        true => {
            transaction
                .fri_prover_jobs_dal()
                .insert_prover_jobs(
                    block_number,
                    blob_urls.circuit_ids_and_urls,
                    AggregationRound::NodeAggregation,
                    depth,
                    protocol_version_id,
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
                    protocol_version_id,
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
                    protocol_version_id,
                )
                .await
        }
    }

    transaction
        .fri_witness_generator_dal()
        .mark_node_aggregation_as_successful(id, started_at.elapsed())
        .await;

    transaction.commit().await.unwrap();
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

    WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::NodeAggregation.into()]
        .observe(started_at.elapsed());

    BlobUrls {
        node_aggregations_url: aggregations_urls,
        circuit_ids_and_urls,
    }
}
