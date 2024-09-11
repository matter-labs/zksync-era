use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

#[derive(Debug)]
pub(crate) struct AggregationBlobUrls {
    pub aggregations_urls: String,
    pub circuit_ids_and_urls: Vec<(u8, String)>,
}

#[derive(Debug)]
pub(crate) struct SchedulerBlobUrls {
    pub circuit_ids_and_urls: Vec<(u8, String)>,
    pub closed_form_inputs_and_urls: Vec<(u8, String, usize)>,
    pub scheduler_witness_url: String,
}

pub(crate) enum BlobUrls {
    Url(String),
    Aggregation(AggregationBlobUrls),
    Scheduler(SchedulerBlobUrls),
}

#[async_trait]
pub(crate) trait ArtifactsManager {
    type Metadata;
    type InputArtifacts;
    type OutputArtifacts;

    async fn get_artifacts(
        metadata: &Self::Medatadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts;

    async fn save_artifacts(
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> BlobUrls;

    async fn update_database(
        connection_pool: ConnectionPool<Prover>,
        block_number: L1BatchNumber,
        started_at: Instant,
    );
}

#[tracing::instrument(skip_all, fields(l1_batch = %block_number))]
pub(super) async fn update_database_basic(
    prover_connection_pool: &ConnectionPool<Prover>,
    started_at: Instant,
    block_number: L1BatchNumber,
    blob_urls: BlobUrls,
) {
    let mut connection = prover_connection_pool
        .connection()
        .await
        .expect("failed to get database connection");
    let mut transaction = connection
        .start_transaction()
        .await
        .expect("failed to get database transaction");
    let protocol_version_id = transaction
        .fri_witness_generator_dal()
        .protocol_version_for_l1_batch(block_number)
        .await;
    transaction
        .fri_prover_jobs_dal()
        .insert_prover_jobs(
            block_number,
            blob_urls.circuit_ids_and_urls,
            AggregationRound::BasicCircuits,
            0,
            protocol_version_id,
        )
        .await;
    transaction
        .fri_witness_generator_dal()
        .create_aggregation_jobs(
            block_number,
            &blob_urls.closed_form_inputs_and_urls,
            &blob_urls.scheduler_witness_url,
            get_recursive_layer_circuit_id_for_base_layer,
            protocol_version_id,
        )
        .await;
    transaction
        .fri_witness_generator_dal()
        .mark_witness_job_as_successful(block_number, started_at.elapsed())
        .await;
    transaction
        .commit()
        .await
        .expect("failed to commit database transaction");
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %block_number, circuit_id = %circuit_id)
)]
async fn update_database_leaf(
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

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    skip_all,
    fields(l1_batch = % block_number, circuit_id = % circuit_id)
)]
async fn update_database_node(
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
                    &blob_urls.aggregations_urls,
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
