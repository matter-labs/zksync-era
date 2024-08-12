use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_definitions::circuit_definitions::recursion_layer::RECURSION_ARITY;
use tokio::sync::Semaphore;
use zkevm_test_harness::witness::recursive_aggregation::{
    compute_node_vk_commitment, create_node_witness,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::recursion_layer::{
            ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey,
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
        save_recursive_layer_prover_input_artifacts, AggregationWrapper, AggregationWrapperLegacy,
    },
};

pub struct NodeAggregationArtifacts {
    circuit_id: u8,
    block_number: L1BatchNumber,
    depth: u16,
    pub next_aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    pub recursive_circuit_ids_and_urls: Vec<(u8, String)>,
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
    aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    proofs_ids: Vec<u32>,
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

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %job.block_number, circuit_id = %job.circuit_id)
    )]
    pub async fn process_job_impl(
        job: NodeAggregationWitnessGeneratorJob,
        started_at: Instant,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
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

        let mut proof_ids_iter = job.proofs_ids.into_iter();
        let mut proofs_ids = vec![];
        for queues in job.aggregations.chunks(RECURSION_ARITY) {
            let proofs_for_chunk: Vec<_> = (&mut proof_ids_iter).take(queues.len()).collect();
            proofs_ids.push(proofs_for_chunk);
        }

        assert_eq!(
            job.aggregations.chunks(RECURSION_ARITY).len(),
            proofs_ids.len()
        );

        let semaphore = Arc::new(Semaphore::new(max_circuits_in_flight));

        let mut handles = vec![];
        for (circuit_idx, (chunk, proofs_ids_for_chunk)) in job
            .aggregations
            .chunks(RECURSION_ARITY)
            .zip(proofs_ids)
            .enumerate()
        {
            let semaphore = semaphore.clone();

            let object_store = object_store.clone();
            let chunk = Vec::from(chunk);
            let vk = vk.clone();
            let all_leafs_layer_params = job.all_leafs_layer_params.clone();

            let handle = tokio::task::spawn(async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .expect("failed to get permit to process queues chunk");

                let proofs = load_proofs_for_job_ids(&proofs_ids_for_chunk, &*object_store).await;
                let mut recursive_proofs = vec![];
                for wrapper in proofs {
                    match wrapper {
                        FriProofWrapper::Base(_) => {
                            panic!(
                                "Expected only recursive proofs for node agg {} {}",
                                job.circuit_id, job.block_number
                            );
                        }
                        FriProofWrapper::Recursive(recursive_proof) => {
                            recursive_proofs.push(recursive_proof)
                        }
                    }
                }

                let (result_circuit_id, recursive_circuit, input_queue) = create_node_witness(
                    &chunk,
                    recursive_proofs,
                    &vk,
                    node_vk_commitment,
                    &all_leafs_layer_params,
                );

                let recursive_circuit_id_and_url = save_recursive_layer_prover_input_artifacts(
                    job.block_number,
                    circuit_idx,
                    vec![recursive_circuit],
                    AggregationRound::NodeAggregation,
                    job.depth + 1,
                    &*object_store,
                    Some(job.circuit_id),
                )
                .await;

                (
                    (result_circuit_id, input_queue),
                    recursive_circuit_id_and_url,
                )
            });

            handles.push(handle);
        }

        let mut next_aggregations = vec![];
        let mut recursive_circuit_ids_and_urls = vec![];
        for handle in handles {
            let (next_aggregation, recursive_circuit_id_and_url) = handle.await.unwrap();

            next_aggregations.push(next_aggregation);
            recursive_circuit_ids_and_urls.extend(recursive_circuit_id_and_url);
        }

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
            recursive_circuit_ids_and_urls,
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
        let object_store = self.object_store.clone();
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::spawn(async move {
            Ok(Self::process_job_impl(job, started_at, object_store, max_circuits_in_flight).await)
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %artifacts.block_number, circuit_id = %artifacts.circuit_id)
    )]
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

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %metadata.block_number, circuit_id = %metadata.circuit_id)
)]
pub async fn prepare_job(
    metadata: NodeAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> anyhow::Result<NodeAggregationWitnessGeneratorJob> {
    let started_at = Instant::now();
    let artifacts = get_artifacts(&metadata, object_store).await;

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

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::NodeAggregation.into()]
        .observe(started_at.elapsed());

    Ok(NodeAggregationWitnessGeneratorJob {
        circuit_id: metadata.circuit_id,
        block_number: metadata.block_number,
        depth: metadata.depth,
        aggregations: artifacts.0,
        proofs_ids: metadata.prover_job_ids_for_proofs,
        leaf_vk,
        node_vk,
        all_leafs_layer_params: get_leaf_vk_params(&keystore).context("get_leaf_vk_params()")?,
    })
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    skip_all,
    fields(l1_batch = %block_number, circuit_id = %circuit_id)
)]
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

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %metadata.block_number, circuit_id = %metadata.circuit_id)
)]
async fn get_artifacts(
    metadata: &NodeAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> AggregationWrapper {
    let key = AggregationsKey {
        block_number: metadata.block_number,
        circuit_id: metadata.circuit_id,
        depth: metadata.depth,
    };
    let result = object_store.get(key).await;

    // TODO: remove after transition
    return match result {
        Ok(aggregation_wrapper) => aggregation_wrapper,
        Err(error) => {
            // probably legacy struct is saved in GCS
            if let ObjectStoreError::Serialization(serialization_error) = error {
                let legacy_wrapper: AggregationWrapperLegacy =
                    object_store.get(key).await.unwrap_or_else(|inner_error| {
                        panic!(
                            "node aggregation job artifacts getting error. Key: {:?}, errors: {:?} {:?}",
                            key, serialization_error, inner_error
                        )
                    });
                AggregationWrapper(legacy_wrapper.0.into_iter().map(|x| (x.0, x.1)).collect())
            } else {
                panic!("node aggregation job artifacts missing: {:?}", key)
            }
        }
    };
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %artifacts.block_number, circuit_id = %artifacts.circuit_id)
)]
async fn save_artifacts(
    artifacts: NodeAggregationArtifacts,
    object_store: &dyn ObjectStore,
) -> BlobUrls {
    let started_at = Instant::now();
    let aggregations_urls = save_node_aggregations_artifacts(
        artifacts.block_number,
        artifacts.circuit_id,
        artifacts.depth,
        artifacts.next_aggregations,
        object_store,
    )
    .await;

    WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::NodeAggregation.into()]
        .observe(started_at.elapsed());

    BlobUrls {
        node_aggregations_url: aggregations_urls,
        circuit_ids_and_urls: artifacts.recursive_circuit_ids_and_urls,
    }
}
