use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use circuit_definitions::circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type;
use tokio::sync::Semaphore;
use zkevm_test_harness::{
    witness::recursive_aggregation::{
        compute_leaf_params, create_leaf_witness, split_recursion_queue,
    },
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zksync_config::configs::FriWitnessGeneratorConfig;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::field::goldilocks::GoldilocksField,
        circuit_definitions::base_layer::{
            ZkSyncBaseLayerClosedFormInput, ZkSyncBaseLayerVerificationKey,
        },
        encodings::recursion_request::RecursionQueueSimulator,
        zkevm_circuits::recursion::leaf_layer::input::RecursionLeafParametersWitness,
    },
    get_current_pod_name,
    keys::ClosedFormInputKey,
    FriProofWrapper,
};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::LeafAggregationJobMetadata, L1BatchNumber,
};
use zksync_vk_setup_data_server_fri::keystore::Keystore;

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    utils::{
        load_proofs_for_job_ids, save_node_aggregations_artifacts,
        save_recursive_layer_prover_input_artifacts, ClosedFormInputWrapper,
    },
};

pub struct LeafAggregationArtifacts {
    circuit_id: u8,
    block_number: L1BatchNumber,
    pub aggregations: Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>,
    pub circuit_ids_and_urls: Vec<(u8, String)>,
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
    pub(crate) proofs_ids: Vec<u32>,
    pub(crate) base_vk: ZkSyncBaseLayerVerificationKey,
    pub(crate) leaf_params: RecursionLeafParametersWitness<GoldilocksField>,
}

#[derive(Debug)]
pub struct LeafAggregationWitnessGenerator {
    config: FriWitnessGeneratorConfig,
    object_store: Arc<dyn ObjectStore>,
    prover_connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
}

impl LeafAggregationWitnessGenerator {
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
        fields(l1_batch = %leaf_job.block_number, circuit_id = %leaf_job.circuit_id)
    )]
    pub async fn process_job_impl(
        leaf_job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
        object_store: Arc<dyn ObjectStore>,
        max_circuits_in_flight: usize,
    ) -> LeafAggregationArtifacts {
        tracing::info!(
            "Starting witness generation of type {:?} for block {} with circuit {}",
            AggregationRound::LeafAggregation,
            leaf_job.block_number.0,
            leaf_job.circuit_id,
        );
        process_leaf_aggregation_job(started_at, leaf_job, object_store, max_circuits_in_flight)
            .await
    }
}

#[async_trait]
impl JobProcessor for LeafAggregationWitnessGenerator {
    type Job = LeafAggregationWitnessGeneratorJob;
    type JobId = u32;
    type JobArtifacts = LeafAggregationArtifacts;

    const SERVICE_NAME: &'static str = "fri_leaf_aggregation_witness_generator";

    async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
        let mut prover_connection = self.prover_connection_pool.connection().await?;
        let pod_name = get_current_pod_name();
        let Some(metadata) = prover_connection
            .fri_witness_generator_dal()
            .get_next_leaf_aggregation_job(self.protocol_version, &pod_name)
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
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .mark_leaf_aggregation_job_failed(&error, job_id)
            .await;
    }

    #[allow(clippy::async_yields_async)]
    async fn process_job(
        &self,
        _job_id: &Self::JobId,
        job: LeafAggregationWitnessGeneratorJob,
        started_at: Instant,
    ) -> tokio::task::JoinHandle<anyhow::Result<LeafAggregationArtifacts>> {
        let object_store = self.object_store.clone();
        let current_handle = tokio::runtime::Handle::current();
        let max_circuits_in_flight = self.config.max_circuits_in_flight;
        tokio::task::spawn_blocking(move || {
            Ok(current_handle.block_on(Self::process_job_impl(
                job,
                started_at,
                object_store,
                max_circuits_in_flight,
            )))
        })
    }

    async fn save_result(
        &self,
        job_id: u32,
        started_at: Instant,
        artifacts: LeafAggregationArtifacts,
    ) -> anyhow::Result<()> {
        let block_number = artifacts.block_number;
        let circuit_id = artifacts.circuit_id;
        tracing::info!(
            "Saving leaf aggregation artifacts for block {} with circuit {}",
            block_number.0,
            circuit_id,
        );
        let blob_urls = save_artifacts(artifacts, &*self.object_store).await;
        tracing::info!(
            "Saved leaf aggregation artifacts for block {} with circuit {} (count: {})",
            block_number.0,
            circuit_id,
            blob_urls.circuit_ids_and_urls.len(),
        );
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
            .connection()
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

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %metadata.block_number, circuit_id = %metadata.circuit_id)
)]
pub async fn prepare_leaf_aggregation_job(
    metadata: LeafAggregationJobMetadata,
    object_store: &dyn ObjectStore,
) -> anyhow::Result<LeafAggregationWitnessGeneratorJob> {
    let started_at = Instant::now();
    let closed_form_input = get_artifacts(&metadata, object_store).await;

    WITNESS_GENERATOR_METRICS.blob_fetch_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

    let started_at = Instant::now();
    let keystore = Keystore::default();
    let base_vk = keystore
        .load_base_layer_verification_key(metadata.circuit_id)
        .context("get_base_layer_vk_for_circuit_type()")?;

    let leaf_circuit_id = base_circuit_type_into_recursive_leaf_circuit_type(
        BaseLayerCircuitType::from_numeric_value(metadata.circuit_id),
    ) as u8;

    let leaf_vk = keystore
        .load_recursive_layer_verification_key(leaf_circuit_id)
        .context("get_recursive_layer_vk_for_circuit_type()")?;
    let leaf_params = compute_leaf_params(metadata.circuit_id, base_vk.clone(), leaf_vk);

    WITNESS_GENERATOR_METRICS.prepare_job_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

    Ok(LeafAggregationWitnessGeneratorJob {
        circuit_id: metadata.circuit_id,
        block_number: metadata.block_number,
        closed_form_inputs: closed_form_input,
        proofs_ids: metadata.prover_job_ids_for_proofs,
        base_vk,
        leaf_params,
    })
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %job.block_number, circuit_id = %job.circuit_id)
)]
pub async fn process_leaf_aggregation_job(
    started_at: Instant,
    job: LeafAggregationWitnessGeneratorJob,
    object_store: Arc<dyn ObjectStore>,
    max_circuits_in_flight: usize,
) -> LeafAggregationArtifacts {
    let circuit_id = job.circuit_id;
    let queues = split_recursion_queue(job.closed_form_inputs.1);

    let aggregations = queues
        .iter()
        .cloned()
        .map(|queue| (circuit_id as u64, queue))
        .collect();

    let mut proof_ids_iter = job.proofs_ids.into_iter();
    let mut proofs_ids = vec![];
    for queue in queues.iter() {
        let proofs_ids_for_queue: Vec<_> = (&mut proof_ids_iter)
            .take(queue.num_items as usize)
            .collect();
        assert_eq!(queue.num_items as usize, proofs_ids_for_queue.len());
        proofs_ids.push(proofs_ids_for_queue);
    }

    let semaphore = Arc::new(Semaphore::new(max_circuits_in_flight));

    let mut handles = vec![];
    for (circuit_idx, (queue, proofs_ids_for_queue)) in
        queues.into_iter().zip(proofs_ids).enumerate()
    {
        let semaphore = semaphore.clone();

        let object_store = object_store.clone();
        let queue = queue.clone();
        let base_vk = job.base_vk.clone();
        let leaf_params = (circuit_id, job.leaf_params.clone());

        let handle = tokio::task::spawn(async move {
            let _permit = semaphore
                .acquire()
                .await
                .expect("failed to get permit to process queues chunk");

            let proofs = load_proofs_for_job_ids(&proofs_ids_for_queue, &*object_store).await;
            let base_proofs = proofs
                .into_iter()
                .map(|wrapper| match wrapper {
                    FriProofWrapper::Base(base_proof) => base_proof,
                    FriProofWrapper::Recursive(_) => {
                        panic!(
                            "Expected only base proofs for leaf agg {} {}",
                            job.circuit_id, job.block_number
                        );
                    }
                })
                .collect();

            let (_, circuit) = create_leaf_witness(
                circuit_id.into(),
                queue,
                base_proofs,
                &base_vk,
                &leaf_params,
            );

            let new_circuit_ids_and_urls = save_recursive_layer_prover_input_artifacts(
                job.block_number,
                circuit_idx,
                vec![circuit],
                AggregationRound::LeafAggregation,
                0,
                &*object_store,
                None,
            )
            .await;

            new_circuit_ids_and_urls
        });

        handles.push(handle);
    }

    let circuit_ids_and_urls_results = futures::future::join_all(handles).await;
    let circuit_ids_and_urls = circuit_ids_and_urls_results
        .into_iter()
        .map(|x| x.unwrap())
        .flatten()
        .collect();

    WITNESS_GENERATOR_METRICS.witness_generation_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

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
        circuit_ids_and_urls,
        closed_form_inputs: job.closed_form_inputs.0,
    }
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %block_number, circuit_id = %circuit_id)
)]
async fn update_database(
    prover_connection_pool: &ConnectionPool<Prover>,
    started_at: Instant,
    block_number: L1BatchNumber,
    job_id: u32,
    blob_urls: BlobUrls,
    circuit_id: u8,
) {
    tracing::info!(
        "Updating database for job_id {}, block {} with circuit id {}",
        job_id,
        block_number.0,
        circuit_id,
    );
    let mut prover_connection = prover_connection_pool.connection().await.unwrap();
    let mut transaction = prover_connection.start_transaction().await.unwrap();
    let number_of_dependent_jobs = blob_urls.circuit_ids_and_urls.len();
    let protocol_version_id = transaction
        .fri_witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await;
    tracing::info!(
        "Inserting {} prover jobs for job_id {}, block {} with circuit id {}",
        blob_urls.circuit_ids_and_urls.len(),
        job_id,
        block_number.0,
        circuit_id,
    );
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
    tracing::info!(
        "Updating node aggregation jobs url for job_id {}, block {} with circuit id {}",
        job_id,
        block_number.0,
        circuit_id,
    );
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
    tracing::info!(
        "Marking leaf aggregation job as successful for job id {}, block {} with circuit id {}",
        job_id,
        block_number.0,
        circuit_id,
    );
    transaction
        .fri_witness_generator_dal()
        .mark_leaf_aggregation_as_successful(job_id, started_at.elapsed())
        .await;

    tracing::info!(
        "Committing transaction for job_id {}, block {} with circuit id {}",
        job_id,
        block_number.0,
        circuit_id,
    );
    transaction.commit().await.unwrap();
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %metadata.block_number, circuit_id = %metadata.circuit_id)
)]
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

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %artifacts.block_number, circuit_id = %artifacts.circuit_id)
)]
async fn save_artifacts(
    artifacts: LeafAggregationArtifacts,
    object_store: &dyn ObjectStore,
) -> BlobUrls {
    let started_at = Instant::now();
    let aggregations_urls = save_node_aggregations_artifacts(
        artifacts.block_number,
        get_recursive_layer_circuit_id_for_base_layer(artifacts.circuit_id),
        0,
        artifacts.aggregations,
        object_store,
    )
    .await;
    WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::LeafAggregation.into()]
        .observe(started_at.elapsed());

    BlobUrls {
        circuit_ids_and_urls: artifacts.circuit_ids_and_urls,
        aggregations_urls,
    }
}
