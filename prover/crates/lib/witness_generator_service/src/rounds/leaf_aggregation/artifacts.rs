use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::keys::{AggregationsKey, ClosedFormInputKey};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{basic_fri_types::AggregationRound, prover_dal::LeafAggregationJobMetadata};

use crate::{
    artifact_manager::{AggregationBlobUrls, ArtifactsManager, JobId},
    rounds::leaf_aggregation::{LeafAggregation, LeafAggregationArtifacts},
    utils::{AggregationWrapper, ClosedFormInputWrapper},
};

#[async_trait]
impl ArtifactsManager for LeafAggregation {
    type InputMetadata = LeafAggregationJobMetadata;
    type InputArtifacts = ClosedFormInputWrapper;
    type OutputArtifacts = LeafAggregationArtifacts;
    type BlobUrls = AggregationBlobUrls;

    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let key = ClosedFormInputKey {
            batch_id: metadata.batch_id,
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
        fields(l1_batch = %artifacts.batch_id, circuit_id = %artifacts.circuit_id)
    )]
    async fn save_to_bucket(
        _job_id: JobId,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> AggregationBlobUrls {
        let key = AggregationsKey {
            batch_id: artifacts.batch_id,
            circuit_id: get_recursive_layer_circuit_id_for_base_layer(artifacts.circuit_id),
            depth: 0,
        };
        let aggregation_urls = object_store
            .put(key, &AggregationWrapper(artifacts.aggregations))
            .await
            .unwrap();

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
        job_id: JobId,
        started_at: Instant,
        blob_urls: AggregationBlobUrls,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Updating database for job_id {}, block {} with circuit id {}",
            job_id,
            artifacts.batch_id,
            artifacts.circuit_id,
        );

        let mut prover_connection = connection_pool.connection().await.unwrap();
        let mut transaction = prover_connection.start_transaction().await.unwrap();
        let number_of_dependent_jobs = blob_urls.circuit_ids_and_urls.len();
        let protocol_version_id = transaction
            .fri_basic_witness_generator_dal()
            .protocol_version_for_l1_batch(artifacts.batch_id)
            .await
            .unwrap();
        let batch_sealed_at = transaction
            .fri_basic_witness_generator_dal()
            .get_batch_sealed_at_timestamp(artifacts.batch_id)
            .await;

        tracing::info!(
            "Inserting {} prover jobs for job_id {}, block {} with circuit id {}",
            blob_urls.circuit_ids_and_urls.len(),
            job_id,
            artifacts.batch_id,
            artifacts.circuit_id,
        );
        transaction
            .fri_prover_jobs_dal()
            .insert_prover_jobs(
                artifacts.batch_id,
                blob_urls.circuit_ids_and_urls,
                AggregationRound::LeafAggregation,
                0,
                protocol_version_id,
                batch_sealed_at,
            )
            .await;
        tracing::info!(
            "Updating node aggregation jobs url for job_id {}, block {} with circuit id {}",
            job_id,
            artifacts.batch_id,
            artifacts.circuit_id,
        );
        transaction
            .fri_node_witness_generator_dal()
            .update_node_aggregation_jobs_url(
                artifacts.batch_id,
                get_recursive_layer_circuit_id_for_base_layer(artifacts.circuit_id),
                number_of_dependent_jobs,
                0,
                blob_urls.aggregation_urls,
            )
            .await;
        tracing::info!(
            "Marking leaf aggregation job as successful for job id {}, block {} with circuit id {}",
            job_id,
            artifacts.batch_id,
            artifacts.circuit_id,
        );
        transaction
            .fri_leaf_witness_generator_dal()
            .mark_leaf_aggregation_as_successful(
                job_id.id(),
                job_id.chain_id(),
                started_at.elapsed(),
            )
            .await;

        tracing::info!(
            "Committing transaction for job_id {}, block {} with circuit id {}",
            job_id,
            artifacts.batch_id,
            artifacts.circuit_id,
        );
        transaction.commit().await?;
        Ok(())
    }
}
