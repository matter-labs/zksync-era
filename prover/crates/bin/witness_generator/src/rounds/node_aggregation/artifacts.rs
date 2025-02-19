use std::time::Instant;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::keys::AggregationsKey;
use zksync_types::{basic_fri_types::AggregationRound, prover_dal::NodeAggregationJobMetadata};

use crate::{
    artifacts::{AggregationBlobUrls, ArtifactsManager},
    metrics::WITNESS_GENERATOR_METRICS,
    rounds::node_aggregation::{NodeAggregation, NodeAggregationArtifacts},
    utils::AggregationWrapper,
};

#[async_trait]
impl ArtifactsManager for NodeAggregation {
    type InputMetadata = NodeAggregationJobMetadata;
    type InputArtifacts = AggregationWrapper;
    type OutputArtifacts = NodeAggregationArtifacts;
    type BlobUrls = AggregationBlobUrls;

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % metadata.block_number, circuit_id = % metadata.circuit_id)
    )]
    async fn get_artifacts(
        metadata: &Self::InputMetadata,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Self::InputArtifacts> {
        let key = AggregationsKey {
            block_number: metadata.block_number,
            circuit_id: metadata.circuit_id,
            depth: metadata.depth,
        };
        let artifacts = object_store.get(key).await.unwrap_or_else(|error| {
            panic!(
                "node aggregation job artifacts getting error. Key: {:?}, error: {:?}",
                key, error
            )
        });

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
        _shall_save_to_public_bucket: bool,
        _public_blob_store: Option<std::sync::Arc<dyn ObjectStore>>,
    ) -> AggregationBlobUrls {
        let started_at = Instant::now();
        let key = AggregationsKey {
            block_number: artifacts.block_number,
            circuit_id: artifacts.circuit_id,
            depth: artifacts.depth,
        };
        let aggregation_urls = object_store
            .put(key, &AggregationWrapper(artifacts.next_aggregations))
            .await
            .unwrap();

        WITNESS_GENERATOR_METRICS.blob_save_time[&AggregationRound::NodeAggregation.into()]
            .observe(started_at.elapsed());

        AggregationBlobUrls {
            aggregation_urls,
            circuit_ids_and_urls: artifacts.recursive_circuit_ids_and_urls,
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % job_id)
    )]
    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: u32,
        started_at: Instant,
        blob_urls: AggregationBlobUrls,
        artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        let mut prover_connection = connection_pool.connection().await.unwrap();
        let mut transaction = prover_connection.start_transaction().await.unwrap();
        let dependent_jobs = blob_urls.circuit_ids_and_urls.len();
        let protocol_version_id = transaction
            .fri_basic_witness_generator_dal()
            .protocol_version_for_l1_batch(artifacts.block_number)
            .await;
        match artifacts.next_aggregations.len() > 1 {
            true => {
                transaction
                    .fri_prover_jobs_dal()
                    .insert_prover_jobs(
                        artifacts.block_number,
                        blob_urls.circuit_ids_and_urls,
                        AggregationRound::NodeAggregation,
                        artifacts.depth,
                        protocol_version_id,
                    )
                    .await;
                transaction
                    .fri_node_witness_generator_dal()
                    .insert_node_aggregation_jobs(
                        artifacts.block_number,
                        artifacts.circuit_id,
                        Some(dependent_jobs as i32),
                        artifacts.depth,
                        &blob_urls.aggregation_urls,
                        protocol_version_id,
                    )
                    .await;
            }
            false => {
                let (_, blob_url) = blob_urls.circuit_ids_and_urls[0].clone();
                transaction
                    .fri_prover_jobs_dal()
                    .insert_prover_job(
                        artifacts.block_number,
                        artifacts.circuit_id,
                        artifacts.depth,
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
            .fri_node_witness_generator_dal()
            .mark_node_aggregation_as_successful(job_id, started_at.elapsed())
            .await;

        transaction.commit().await?;

        Ok(())
    }
}
