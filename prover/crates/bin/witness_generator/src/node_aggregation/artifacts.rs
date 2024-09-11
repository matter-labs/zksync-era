use std::time::Instant;

use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::keys::AggregationsKey;
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::NodeAggregationJobMetadata, L1BatchNumber,
};

use crate::{
    metrics::WITNESS_GENERATOR_METRICS,
    node_aggregation::{NodeAggregationArtifacts, NodeAggregationWitnessGenerator},
    traits::{AggregationBlobUrls, ArtifactsManager, BlobUrls},
    utils::{save_node_aggregations_artifacts, AggregationWrapper},
};

impl ArtifactsManager for NodeAggregationWitnessGenerator {
    type Metadata = NodeAggregationJobMetadata;
    type InputArtifacts = AggregationWrapper;
    type OutputArtifacts = NodeAggregationArtifacts;

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = % metadata.block_number, circuit_id = % metadata.circuit_id)
    )]
    async fn get_artifacts(
        metadata: &Self::Metadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts {
        let key = AggregationsKey {
            block_number: metadata.block_number,
            circuit_id: metadata.circuit_id,
            depth: metadata.depth,
        };
        object_store.get(key).await.unwrap_or_else(|error| {
            panic!(
                "node aggregation job artifacts getting error. Key: {:?}, error: {:?}",
                key, error
            )
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(l1_batch = %artifacts.block_number, circuit_id = %artifacts.circuit_id)
    )]
    async fn save_artifacts(
        artifacts: Self::OutputArtifacts,
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

        BlobUrls::Aggregation(AggregationBlobUrls {
            aggregations_urls,
            circuit_ids_and_urls: artifacts.recursive_circuit_ids_and_urls,
        })
    }

    async fn update_database(
        connection_pool: ConnectionPool<Prover>,
        block_number: L1BatchNumber,
        started_at: Instant,
    ) {
        todo!()
    }
}
