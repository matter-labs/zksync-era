use std::{
    io::{BufWriter, Write as _},
    sync::Arc,
};

use circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerCircuit;
use once_cell::sync::Lazy;
use zkevm_test_harness::boojum::field::goldilocks::GoldilocksField;
use zksync_circuit_prover_service::types::circuit_wrapper::CircuitWrapper;
use zksync_multivm::utils::get_used_bootloader_memory_bytes;
use zksync_object_store::{serialize_using_bincode, Bucket, ObjectStore, StoredObject};
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
    },
    keys::{AggregationsKey, ClosedFormInputKey, FriCircuitKey},
    FriProofWrapper,
};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchId, ProtocolVersionId, U256};

use crate::artifact_manager::JobId;

// Creates a temporary file with the serialized KZG setup usable by `zkevm_test_harness` functions.
pub(crate) static KZG_TRUSTED_SETUP_FILE: Lazy<tempfile::NamedTempFile> = Lazy::new(|| {
    let mut file = tempfile::NamedTempFile::new().expect("cannot create file for KZG setup");
    BufWriter::new(file.as_file_mut())
        .write_all(include_bytes!("trusted_setup.json"))
        .expect("failed writing KZG trusted setup to temporary file");
    file
});

pub fn expand_bootloader_contents(
    packed: &[(usize, U256)],
    protocol_version: ProtocolVersionId,
) -> Vec<u8> {
    let full_length = get_used_bootloader_memory_bytes(protocol_version.into());

    let mut result = vec![0u8; full_length];

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

    fn fallback_key(key: Self::Key<'_>) -> Option<String> {
        Some(format!(
            "closed_form_inputs_{batch_number}_{circuit_id}.bin",
            batch_number = key.batch_id.batch_number().0,
            circuit_id = key.circuit_id
        ))
    }

    fn encode_key(key: Self::Key<'_>) -> String {
        let ClosedFormInputKey {
            batch_id,
            circuit_id,
        } = key;
        format!(
            "closed_form_inputs_{batch_number}_{chain_id}_{circuit_id}.bin",
            batch_number = batch_id.batch_number().0,
            chain_id = batch_id.chain_id().inner()
        )
    }

    serialize_using_bincode!();
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AggregationWrapper(pub Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>);

impl StoredObject for AggregationWrapper {
    const BUCKET: Bucket = Bucket::NodeAggregationWitnessJobsFri;
    type Key<'a> = AggregationsKey;

    fn fallback_key(key: Self::Key<'_>) -> Option<String> {
        let AggregationsKey {
            batch_id,
            circuit_id,
            depth,
        } = key;
        Some(format!(
            "aggregations_{block_number}_{circuit_id}_{depth}.bin",
            block_number = batch_id.batch_number().0
        ))
    }

    fn encode_key(key: Self::Key<'_>) -> String {
        let AggregationsKey {
            batch_id,
            circuit_id,
            depth,
        } = key;

        format!(
            "aggregations_{block_number}_{chain_id}_{circuit_id}_{depth}.bin",
            block_number = batch_id.batch_number().0,
            chain_id = batch_id.chain_id().inner(),
        )
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
    type Key<'a> = L1BatchId;

    fn fallback_key(key: Self::Key<'_>) -> Option<String> {
        Some(format!(
            "scheduler_witness_{batch_number}.bin",
            batch_number = key.batch_number().0
        ))
    }

    fn encode_key(key: Self::Key<'_>) -> String {
        format!(
            "scheduler_witness_{batch_number}_{chain_id}.bin",
            batch_number = key.batch_number().0,
            chain_id = key.chain_id().inner()
        )
    }

    serialize_using_bincode!();
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %batch_id, circuit_id = %circuit.numeric_circuit_type())
)]
pub async fn save_circuit(
    batch_id: L1BatchId,
    circuit: ZkSyncBaseLayerCircuit,
    sequence_number: usize,
    object_store: Arc<dyn ObjectStore>,
) -> (u8, String) {
    let circuit_id = circuit.numeric_circuit_type();
    let circuit_key = FriCircuitKey {
        batch_id,
        sequence_number,
        circuit_id,
        aggregation_round: AggregationRound::BasicCircuits,
        depth: 0,
    };

    let blob_url = object_store
        .put(circuit_key, &CircuitWrapper::Base(circuit))
        .await
        .unwrap();

    (circuit_id, blob_url)
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %batch_id)
)]
pub async fn save_recursive_layer_prover_input_artifacts(
    batch_id: L1BatchId,
    sequence_number_offset: usize,
    recursive_circuits: Vec<ZkSyncRecursiveLayerCircuit>,
    aggregation_round: AggregationRound,
    depth: u16,
    object_store: &dyn ObjectStore,
    base_layer_circuit_id: Option<u8>,
) -> Vec<(u8, usize, String)> {
    let mut ids_sequence_numbers_and_urls = Vec::with_capacity(recursive_circuits.len());
    for (sequence_number, circuit) in recursive_circuits.into_iter().enumerate() {
        let circuit_id = base_layer_circuit_id.unwrap_or_else(|| circuit.numeric_circuit_type());
        let sequence_number = sequence_number_offset + sequence_number;
        let circuit_key = FriCircuitKey {
            batch_id,
            sequence_number,
            circuit_id,
            aggregation_round,
            depth,
        };
        let blob_url = object_store
            .put(circuit_key, &CircuitWrapper::Recursive(circuit))
            .await
            .unwrap();
        ids_sequence_numbers_and_urls.push((circuit_id, sequence_number, blob_url));
    }
    ids_sequence_numbers_and_urls
}

#[tracing::instrument(skip_all)]
pub async fn load_proofs_for_job_ids(
    job_ids: &[JobId],
    object_store: &dyn ObjectStore,
) -> Vec<FriProofWrapper> {
    let mut handles = Vec::with_capacity(job_ids.len());
    for job_id in job_ids {
        handles.push(object_store.get((job_id.id(), job_id.chain_id())));
    }
    futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|x| x.unwrap())
        .collect()
}
