use std::time::Instant;
use circuit_definitions::zkevm_circuits::scheduler::block_header::BlockAuxilaryOutputWitness;
use circuit_definitions::zkevm_circuits::scheduler::input::SchedulerCircuitInstanceWitness;
use zksync_multivm::circuit_sequencer_api_latest::boojum::field::goldilocks::{GoldilocksExt2, GoldilocksField};
use zksync_multivm::circuit_sequencer_api_latest::boojum::gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge;
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::AuxOutputWitnessWrapper;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::basic_fri_types::AggregationRound;
use zksync_types::L1BatchNumber;
use crate::leaf_aggregation::LeafAggregationArtifacts;
use crate::metrics::WITNESS_GENERATOR_METRICS;
use crate::node_aggregation::NodeAggregationArtifacts;
use crate::utils::{save_node_aggregations_artifacts, SchedulerPartialInputWrapper};

#[derive(Debug)]
pub(crate) struct BlobUrls {
    pub aggregations_urls: String,
    pub circuit_ids_and_urls: Vec<(u8, String)>,
}

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
}

async fn save_artifacts_leaf(
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
        aggregations_urls,
        circuit_ids_and_urls: artifacts.circuit_ids_and_urls,
    }
}

async fn save_artifacts_node(
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
        aggregations_urls,
        circuit_ids_and_urls: artifacts.recursive_circuit_ids_and_urls,
    }
}
