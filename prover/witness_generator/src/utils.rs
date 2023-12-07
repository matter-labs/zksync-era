use zkevm_test_harness::{
    boojum::field::goldilocks::GoldilocksField, witness::full_block_artifact::BlockBasicCircuits,
};
use zksync_object_store::{
    serialize_using_bincode, AggregationsKey, Bucket, ClosedFormInputKey, FriCircuitKey,
    ObjectStore, StoredObject,
};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            field::goldilocks::GoldilocksExt2,
            gadgets::recursion::recursive_tree_hasher::CircuitGoldilocksPoseidon2Sponge,
        },
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerClosedFormInput,
            recursion_layer::ZkSyncRecursiveLayerCircuit,
        },
        encodings::recursion_request::RecursionQueueSimulator,
        zkevm_circuits::scheduler::input::SchedulerCircuitInstanceWitness,
        ZkSyncDefaultRoundFunction,
    },
    CircuitWrapper, FriProofWrapper,
};
use zksync_system_constants::USED_BOOTLOADER_MEMORY_BYTES;
use zksync_types::{proofs::AggregationRound, L1BatchNumber, U256};

pub fn expand_bootloader_contents(packed: &[(usize, U256)]) -> Vec<u8> {
    let mut result = vec![0u8; USED_BOOTLOADER_MEMORY_BYTES];

    for (offset, value) in packed {
        value.to_big_endian(&mut result[(offset * 32)..(offset + 1) * 32]);
    }

    result
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ClosedFormInputWrapper(
    pub(crate) Vec<ZkSyncBaseLayerClosedFormInput<GoldilocksField>>,
    pub(crate) RecursionQueueSimulator<GoldilocksField>,
);

impl StoredObject for ClosedFormInputWrapper {
    const BUCKET: Bucket = Bucket::LeafAggregationWitnessJobsFri;
    type Key<'a> = ClosedFormInputKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        let ClosedFormInputKey {
            block_number,
            circuit_id,
        } = key;
        format!("closed_form_inputs_{block_number}_{circuit_id}.bin")
    }

    serialize_using_bincode!();
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AggregationWrapper(
    pub  Vec<(
        u64,
        RecursionQueueSimulator<GoldilocksField>,
        ZkSyncRecursiveLayerCircuit,
    )>,
);

impl StoredObject for AggregationWrapper {
    const BUCKET: Bucket = Bucket::NodeAggregationWitnessJobsFri;
    type Key<'a> = AggregationsKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        let AggregationsKey {
            block_number,
            circuit_id,
            depth,
        } = key;
        format!("aggregations_{block_number}_{circuit_id}_{depth}.bin")
    }

    serialize_using_bincode!();
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SchedulerPartialInputWrapper(
    pub  SchedulerCircuitInstanceWitness<
        GoldilocksField,
        CircuitGoldilocksPoseidon2Sponge,
        GoldilocksExt2,
    >,
);

impl StoredObject for SchedulerPartialInputWrapper {
    const BUCKET: Bucket = Bucket::SchedulerWitnessJobsFri;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("scheduler_witness_{key}.bin")
    }

    serialize_using_bincode!();
}

pub async fn save_base_prover_input_artifacts(
    block_number: L1BatchNumber,
    circuits: BlockBasicCircuits<GoldilocksField, ZkSyncDefaultRoundFunction>,
    object_store: &dyn ObjectStore,
    aggregation_round: AggregationRound,
) -> Vec<(u8, String)> {
    let circuits = circuits.into_flattened_set();
    let mut ids_and_urls = Vec::with_capacity(circuits.len());
    for (sequence_number, circuit) in circuits.into_iter().enumerate() {
        let circuit_id = circuit.numeric_circuit_type();
        let circuit_key = FriCircuitKey {
            block_number,
            sequence_number,
            circuit_id,
            aggregation_round,
            depth: 0,
        };
        let blob_url = object_store
            .put(circuit_key, &CircuitWrapper::Base(circuit))
            .await
            .unwrap();
        ids_and_urls.push((circuit_id, blob_url));
    }
    ids_and_urls
}

pub async fn save_recursive_layer_prover_input_artifacts(
    block_number: L1BatchNumber,
    aggregations: Vec<(
        u64,
        RecursionQueueSimulator<GoldilocksField>,
        ZkSyncRecursiveLayerCircuit,
    )>,
    aggregation_round: AggregationRound,
    depth: u16,
    object_store: &dyn ObjectStore,
    base_layer_circuit_id: Option<u8>,
) -> Vec<(u8, String)> {
    let mut ids_and_urls = Vec::with_capacity(aggregations.len());
    for (sequence_number, (_, _, circuit)) in aggregations.into_iter().enumerate() {
        let circuit_id = base_layer_circuit_id.unwrap_or_else(|| circuit.numeric_circuit_type());
        let circuit_key = FriCircuitKey {
            block_number,
            sequence_number,
            circuit_id,
            aggregation_round,
            depth,
        };
        let blob_url = object_store
            .put(circuit_key, &CircuitWrapper::Recursive(circuit))
            .await
            .unwrap();
        ids_and_urls.push((circuit_id, blob_url));
    }
    ids_and_urls
}

pub async fn save_node_aggregations_artifacts(
    block_number: L1BatchNumber,
    circuit_id: u8,
    depth: u16,
    aggregations: Vec<(
        u64,
        RecursionQueueSimulator<GoldilocksField>,
        ZkSyncRecursiveLayerCircuit,
    )>,
    object_store: &dyn ObjectStore,
) -> String {
    let key = AggregationsKey {
        block_number,
        circuit_id,
        depth,
    };
    object_store
        .put(key, &AggregationWrapper(aggregations))
        .await
        .unwrap()
}

pub async fn load_proofs_for_job_ids(
    job_ids: &[u32],
    object_store: &dyn ObjectStore,
) -> Vec<FriProofWrapper> {
    let mut proofs = Vec::with_capacity(job_ids.len());
    for &job_id in job_ids {
        proofs.push(object_store.get(job_id).await.unwrap());
    }
    proofs
}
