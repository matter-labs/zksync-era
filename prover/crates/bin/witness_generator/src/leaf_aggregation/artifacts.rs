use std::time::Instant;

use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::keys::ClosedFormInputKey;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::LeafAggregationJobMetadata, L1BatchNumber,
};

use crate::{
    leaf_aggregation::{LeafAggregationArtifacts, LeafAggregationWitnessGenerator},
    metrics::WITNESS_GENERATOR_METRICS,
    traits::{AggregationBlobUrls, ArtifactsManager, BlobUrls},
    utils::{save_node_aggregations_artifacts, ClosedFormInputWrapper},
};

impl ArtifactsManager for LeafAggregationWitnessGenerator {
    type Metadata = LeafAggregationJobMetadata;
    type InputArtifacts = ClosedFormInputWrapper;
    type OutputArtifacts = LeafAggregationArtifacts;

    async fn get_artifacts(
        metadata: &Self::Metadata,
        object_store: &dyn ObjectStore,
    ) -> Self::InputArtifacts {
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
        artifacts: Self::OutputArtifacts,
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

        BlobUrls::Aggregation(AggregationBlobUrls {
            aggregations_urls,
            circuit_ids_and_urls: artifacts.circuit_ids_and_urls,
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
