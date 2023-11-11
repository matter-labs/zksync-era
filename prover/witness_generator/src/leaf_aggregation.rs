use zkevm_test_harness::witness::recursive_aggregation::{
    compute_leaf_params, create_leaf_witnesses,
};

use anyhow::Context as _;
use std::time::Instant;

use async_trait::async_trait;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::{
    ZkSyncBaseLayerClosedFormInput, ZkSyncBaseLayerProof, ZkSyncBaseLayerVerificationKey,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursiveLayerCircuit;
use zksync_prover_fri_types::circuit_definitions::encodings::recursion_request::RecursionQueueSimulator;
use zksync_prover_fri_types::{get_current_pod_name, FriProofWrapper};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_vk_setup_data_server_fri::{
    get_base_layer_vk_for_circuit_type, get_recursive_layer_vk_for_circuit_type,
};

use crate::utils::{
    load_proofs_for_job_ids, save_node_aggregations_artifacts,
    save_recursive_layer_prover_input_artifacts, ClosedFormInputWrapper,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ClosedFormInputKey, ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::{AggregationRound, LeafAggregationJobMetadata};
use zksync_types::protocol_version::FriProtocolVersionId;
use zksync_types::L1BatchNumber;

pub struct LeafAggregationArtifacts {
    circuit_id: u8,
    block_number: L1BatchNumber,
    pub aggregations: Vec<(
        u64,
        RecursionQueueSimulator<GoldilocksField>,
        ZkSyncRecursiveLayerCircuit,
    )>,
    #[allow(dead_code)]
    closed_form_inputs: Vec<ZkSyncBaseLayerClosedFormInput<GoldilocksField>>,
}

#[derive(Debug)]
struct BlobUrls {
    circuit_ids_and_urls: Vec<(u8, String)>,
    aggregations_urls: String,
}

pub struct LeafAggregationWitnessGeneratorJob {
    pub(crate) circuit_id: u8,
    pub(crate) block_number: L1BatchNumber,
    pub(crate) closed_form_inputs: ClosedFormInputWrapper,
    pub(crate) proofs: Vec<ZkSyncBaseLayerProof>,
    pub(crate) base_vk: ZkSyncBaseLayerVerificationKey,
    pub(crate) leaf_params: RecursionLeafParametersWitness<GoldilocksField>,
}

#[derive(Debug)]
pub struct LeafAggregationWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Box<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool,
    protocol_versions: Vec<FriProtocolVersionId>,
}

impl LeafAggregationWitnessGenerator {
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
        leaf_job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> LeafAggregationArtifacts {
        tracing::info!(
            "Starting witness generation of type {:?} for block {} with circuit {}",
            AggregationRound::LeafAggregation,
            leaf_job.block_number.0,
            leaf_job.circuit_id,
        );
        process_leaf_aggregation_job(started_at, leaf_job)
    }
}

#[async_trait]
impl JobProcessor for LeafAggregationWitnessGenerator {
    type Job = LeafAggregationWitnessGeneratorJob;
    type JobId = u32;
    type JobArtifacts = LeafAggregationArtifacts;

    const SERVICE_NAME: &'static str = "fri_leaf_aggregation_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.access_storage().await.unwrap();
        let pod_name = get_current_pod_name();
        let Some(metadata) = prover_connection
            .fri_witness_generator_dal()
            .get_next_leaf_aggregation_job(&self.protocol_versions, &pod_name)
            .await
        else {
            return Ok(None);
        };
        tracing::info!("Processing leaf aggregation job {:?}", metadata.id);
        Ok(Some((
            metadata.id,
            prepare_leaf_aggregation_job(metadata, &*self.object_store)
                .await
                .context("prepare_leaf_aggregation_job()")?,
        )))
    }

    async fn save_failure(&self, job_id: u32, _started_at: Instant, error: String) -> () {
        self.prover_connection_pool
            .access_storage()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_leaf_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<LeafAggregationArtifacts>> {
        tokio::task::spawn_blocking(move || Ok(Self::process_job_sync(job, started_at)))
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: LeafAggregationArtifacts,
    ) -> anyhow::Result<()> {
        let block_number = artifacts.block_number;
        let circuit_id = artifacts.circuit_id;
        let blob_urls = save_artifacts(artifacts, &*self.object_store).await;
        update_database(
            &self.prover_connection_pool,
            started_at,
            block_number,
            job_id,
            blob_urls,
            circuit_id,
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
            .access_storage()
            .await
            .context("failed to acquire DB connection for LeafAggregationWitnessGenerator")?;
        prover_storage
            .fri_witness_generator_dal()
            .get_leaf_aggregation_job_attempts(*job_id)
            .await
            .map(|attempts| attempts.unwrap_or(0))
            .context("failed to get job attempts for LeafAggregationWitnessGenerator")
    }
}

pub async fn prepare_leaf_aggregation_job(
    metadata: LeafAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> anyhow::Result<LeafAggregationWitnessGeneratorJob> {
    let started_at = Instant::now();
    let closed_form_input = get_artifacts(&metadata, object_store).await;
    let proofs = load_proofs_for_job_ids(&metadata.prover_job_ids_for_proofs, object_store).await;
    metrics::histogram!(
        "prover_fri.witness_generation.blob_fetch_time",
        started_at.elapsed(),
        "aggregation_round" => format!("{:?}", AggregationRound::LeafAggregation),
    );
    let started_at = Instant::now();
    let base_vk = get_base_layer_vk_for_circuit_type(metadata.circuit_id)
        .context("get_base_layer_vk_for_circuit_type()")?;
    // this is a temp solution to unblock shadow proving.
    // we should have a method that converts basic circuit id to leaf circuit id as they are different.
    let leaf_vk = get_recursive_layer_vk_for_circuit_type(metadata.circuit_id + 2)
        .context("get_recursive_layer_vk_for_circuit_type()")?;
    let mut base_proofs = vec![];
    for wrapper in proofs {
        match wrapper {
            FriProofWrapper::Base(base_proof) => base_proofs.push(base_proof),
            FriProofWrapper::Recursive(_) => {
                anyhow::bail!("Expected only base proofs for leaf agg {}", metadata.id);
            }
        }
    }
    let leaf_params = compute_leaf_params(metadata.circuit_id, base_vk.clone(), leaf_vk);
    metrics::histogram!(
        "prover_fri.witness_generation.prepare_job_time",
        started_at.elapsed(),
        "aggregation_round" => format!("{:?}", AggregationRound::LeafAggregation),
    );
    Ok(LeafAggregationWitnessGeneratorJob {
        circuit_id: metadata.circuit_id,
        block_number: metadata.block_number,
        closed_form_inputs: closed_form_input,
        proofs: base_proofs,
        base_vk,
        leaf_params,
    })
}

pub fn process_leaf_aggregation_job(
    started_at: Instant,
    job: LeafAggregationWitnessGeneratorJob,
) -> LeafAggregationArtifacts {
    let circuit_id = job.circuit_id;
    let subsets = (
        circuit_id as u64,
        job.closed_form_inputs.1,
        job.closed_form_inputs.0,
    );
    let leaf_params = (circuit_id, job.leaf_params);
    let (aggregations, closed_form_inputs) =
        create_leaf_witnesses(subsets, job.proofs, job.base_vk, leaf_params);
    metrics::histogram!(
        "prover_fri.witness_generation.witness_generation_time",
        started_at.elapsed(),
        "aggregation_round" => format!("{:?}", AggregationRound::LeafAggregation),
    );
    tracing::info!(
        "Leaf witness generation for block {} with circuit id {}: is complete in {:?}.",
        job.block_number.0,
        circuit_id,
        started_at.elapsed(),
    );

    LeafAggregationArtifacts {
        circuit_id,
        block_number: job.block_number,
        aggregations,
        closed_form_inputs,
    }
}

async fn update_database(
    prover_connection_pool: &ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    job_id: u32,
    blob_urls: BlobUrls,
    circuit_id: u8,
) {
    let mut prover_connection = prover_connection_pool.access_storage().await.unwrap();
    let mut transaction = prover_connection.start_transaction().await.unwrap();
    let number_of_dependent_jobs = blob_urls.circuit_ids_and_urls.len();
    let protocol_version_id = transaction
        .fri_witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await;
    transaction
        .fri_prover_jobs_dal()
        .insert_prover_jobs(
            block_number,
            blob_urls.circuit_ids_and_urls,
            AggregationRound::LeafAggregation,
            0,
            protocol_version_id,
        )
        .await;
    transaction
        .fri_witness_generator_dal()
        .update_node_aggregation_jobs_url(
            block_number,
            get_recursive_layer_circuit_id_for_base_layer(circuit_id),
            number_of_dependent_jobs,
            0,
            blob_urls.aggregations_urls,
        )
        .await;
    transaction
        .fri_witness_generator_dal()
        .mark_leaf_aggregation_as_successful(job_id, started_at.elapsed())
        .await;

    transaction.commit().await.unwrap();
}

async fn get_artifacts(
    metadata: &LeafAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> ClosedFormInputWrapper {
    let key = ClosedFormInputKey {
        block_number: metadata.block_number,
        circuit_id: metadata.circuit_id,
    };
    object_store
        .get(key)
        .await
        .unwrap_or_else(|_| panic!("leaf aggregation job artifacts missing: {:?}", key))
}

async fn save_artifacts(
    artifacts: LeafAggregationArtifacts,
    object_store: &dyn ObjectStore,
) -> BlobUrls {
    let started_at = Instant::now();
    let aggregations_urls = save_node_aggregations_artifacts(
        artifacts.block_number,
        get_recursive_layer_circuit_id_for_base_layer(artifacts.circuit_id),
        0,
        artifacts.aggregations.clone(),
        object_store,
    )
    .await;
    let circuit_ids_and_urls = save_recursive_layer_prover_input_artifacts(
        artifacts.block_number,
        artifacts.aggregations,
        AggregationRound::LeafAggregation,
        0,
        object_store,
        None,
    )
    .await;
    metrics::histogram!(
        "prover_fri.witness_generation.blob_save_time",
        started_at.elapsed(),
        "aggregation_round" => format!("{:?}", AggregationRound::LeafAggregation),
    );
    BlobUrls {
        circuit_ids_and_urls,
        aggregations_urls,
    }
}
