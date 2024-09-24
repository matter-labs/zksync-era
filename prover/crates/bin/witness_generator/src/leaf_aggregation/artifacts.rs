use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::keys::{AggregationsKey, ClosedFormInputKey};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{basic_fri_types::AggregationRound, prover_dal::LeafAggregationJobMetadata};

use crate::{
    artifacts::{AggregationBlobUrls, ArtifactsManager},
    leaf_aggregation::{LeafAggregationArtifacts, LeafAggregationWitnessGenerator},
    metrics::WITNESS_GENERATOR_METRICS,
    utils::{AggregationWrapper, ClosedFormInputWrapper},
};

#[async_trait]
impl ArtifactsManager for LeafAggregationWitnessGenerator {
    type InputMetadata = LeafAggregationJobMetadata;
    type InputArtifacts = ClosedFormInputWrapper;
    type OutputArtifacts = LeafAggregationArtifacts;
    type BlobUrls = AggregationBlobUrls;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let key = ClosedFormInputKey {
            block_number: metadata.block_number,
            circuit_id: metadata.circuit_id,
        };

        let artifacts = object_store
            .get(key)
            .await
            .unwrap_or_else(|_| panic!("leaf aggregation job artifacts missing: {:?}", key));

        Ok(artifacts)
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %artifacts.block_number, circuit_id = %artifacts.circuit_id)
    )]
    async fn save_to_bucket(
        _job_id: u32,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> AggregationBlobUrls {
        let started_at = Instant::now();
        let key = AggregationsKey {
            block_number: artifacts.block_number,
            circuit_id: get_recursive_layer_circuit_id_for_base_layer(artifacts.circuit_id),
            depth: 0,
        };
        let aggregation_urls = object_store
            .put(key, &AggregationWrapper(artifacts.aggregations))
            .await
            .unwrap();

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::LeafAggregation.into()]
            .observe(started_at.elapsed());

        AggregationBlobUrls {
            aggregation_urls,
            circuit_ids_and_urls: artifacts.circuit_ids_and_urls,
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %job_id)
    )]
    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
        started_at: Instant,
        blob_urls: AggregationBlobUrls,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Updating database for job_id {}, block {} with circuit id {}",
            job_id,
            artifacts.block_number.0,
            artifacts.circuit_id,
        );

        let mut prover_connection = connection_pool.connection().await.unwrap();
        let mut transaction = prover_connection.start_transaction().await.unwrap();
        let number_of_dependent_jobs = blob_urls.circuit_ids_and_urls.len();
        let protocol_version_id = transaction
            .fri_witness_generator_dal()
            .protocol_version_for_l1_batch(artifacts.block_number)
            .await;
        tracing::info!(
            "Inserting {} prover jobs for job_id {}, block {} with circuit id {}",
            blob_urls.circuit_ids_and_urls.len(),
            job_id,
            artifacts.block_number.0,
            artifacts.circuit_id,
        );
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_jobs(
                artifacts.block_number,
                blob_urls.circuit_ids_and_urls,
                AggregationRound::LeafAggregation,
                0,
                protocol_version_id,
            )
            .await;
        tracing::info!(
            "Updating node aggregation jobs url for job_id {}, block {} with circuit id {}",
            job_id,
            artifacts.block_number.0,
            artifacts.circuit_id,
        );
        transaction
            .fri_witness_generator_dal()
            .update_node_aggregation_jobs_url(
                artifacts.block_number,
                get_recursive_layer_circuit_id_for_base_layer(artifacts.circuit_id),
                number_of_dependent_jobs,
                0,
                blob_urls.aggregation_urls,
            )
            .await;
        tracing::info!(
            "Marking leaf aggregation job as successful for job id {}, block {} with circuit id {}",
            job_id,
            artifacts.block_number.0,
            artifacts.circuit_id,
        );
        transaction
            .fri_witness_generator_dal()
            .mark_leaf_aggregation_as_successful(job_id, started_at.elapsed())
            .await;

        tracing::info!(
            "Committing transaction for job_id {}, block {} with circuit id {}",
            job_id,
            artifacts.block_number.0,
            artifacts.circuit_id,
        );
        transaction.commit().await?;
        Ok(())
    }
}
