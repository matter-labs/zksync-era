use std::{
    io::{BufWriter, Write as _},
    sync::Arc,
};

use circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerCircuit;
use once_cell::sync::Lazy;
use zkevm_test_harness::boojum::field::goldilocks::GoldilocksField;
use zksync_multivm::utils::get_used_bootloader_memory_bytes;
use zksync_object_store::{
    serialize_using_bincode, Bucket, ObjectStore, ObjectStoreError, StoredObject,
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
    },
    keys::{AggregationsKey, ClosedFormInputKey, FriCircuitKey},
    CircuitWrapper, FriProofWrapper,
};
use zksync_types::{
    basic_fri_types::AggregationRound, ChainAwareL1BatchNumber, L1BatchNumber, L2ChainId,
    ProtocolVersionId, U256,
};

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

impl ClosedFormInputWrapper {
    pub async fn conditional_get_from_object_store(
        blob_store: &dyn ObjectStore,
        key: <Self as StoredObject>::Key<'_>,
    ) -> Result<Self, ObjectStoreError> {
        match blob_store.get(key).await {
            Ok(proof) => Ok(proof),
            Err(_) => {
                // If the proof with chain id was not found, we try to fetch the one without chain id
                let mut zero_chain_id_key = key;
                zero_chain_id_key.batch_id.chain_id = L2ChainId::zero();

                blob_store.get(zero_chain_id_key).await
            }
        }
    }
}

impl StoredObject for ClosedFormInputWrapper {
    const BUCKET: Bucket = Bucket::LeafAggregationWitnessJobsFri;
    type Key<'a> = ClosedFormInputKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        let ClosedFormInputKey {
            batch_id,
            circuit_id,
        } = key;

        if batch_id.raw_chain_id() == 0 {
            return format!(
                "closed_form_inputs_{}_{circuit_id}.bin",
                batch_id.raw_batch_number()
            );
        }

        format!(
            "closed_form_inputs_{}_{}_{circuit_id}.bin",
            batch_id.raw_chain_id(),
            batch_id.raw_batch_number(),
        )
    }

    serialize_using_bincode!();
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AggregationWrapper(pub Vec<(u64, RecursionQueueSimulator<GoldilocksField>)>);

impl AggregationWrapper {
    pub async fn conditional_get_from_object_store(
        blob_store: &dyn ObjectStore,
        key: <Self as StoredObject>::Key<'_>,
    ) -> Result<Self, ObjectStoreError> {
        match blob_store.get(key).await {
            Ok(proof) => Ok(proof),
            Err(_) => {
                // If the proof with chain id was not found, we try to fetch the one without chain id
                let mut zero_chain_id_key = key;
                zero_chain_id_key.batch_id.chain_id = L2ChainId::zero();

                blob_store.get(zero_chain_id_key).await
            }
        }
    }
}

impl StoredObject for AggregationWrapper {
    const BUCKET: Bucket = Bucket::NodeAggregationWitnessJobsFri;
    type Key<'a> = AggregationsKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        let AggregationsKey {
            batch_id,
            circuit_id,
            depth,
        } = key;

        if batch_id.raw_chain_id() == 0 {
            return format!(
                "aggregations_{}_{}_{}.bin",
                batch_id.raw_batch_number(),
                circuit_id,
                depth
            );
        }

        format!(
            "aggregations_{}_{}_{circuit_id}_{depth}.bin",
            batch_id.raw_chain_id(),
            batch_id.raw_batch_number(),
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
    type Key<'a> = (L2ChainId, L1BatchNumber);

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("scheduler_witness_{}_{}.bin", key.0.as_u64(), key.1)
    }

    serialize_using_bincode!();
}

#[tracing::instrument(
    skip_all,
    fields(l1_batch = %batch_id.batch_number, circuit_id = %circuit.numeric_circuit_type())
)]
pub async fn save_circuit(
    batch_id: ChainAwareL1BatchNumber,
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

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    skip_all,
    fields(l1_batch = %batch_id.batch_number)
)]
pub async fn save_recursive_layer_prover_input_artifacts(
    batch_id: ChainAwareL1BatchNumber,
    sequence_number_offset: usize,
    recursive_circuits: Vec<ZkSyncRecursiveLayerCircuit>,
    aggregation_round: AggregationRound,
    depth: u16,
    object_store: &dyn ObjectStore,
    base_layer_circuit_id: Option<u8>,
) -> Vec<(u8, String)> {
    let mut ids_and_urls = Vec::with_capacity(recursive_circuits.len());
    for (sequence_number, circuit) in recursive_circuits.into_iter().enumerate() {
        let circuit_id = base_layer_circuit_id.unwrap_or_else(|| circuit.numeric_circuit_type());
        let circuit_key = FriCircuitKey {
            batch_id,
            sequence_number: sequence_number_offset + sequence_number,
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

#[tracing::instrument(skip_all)]
pub async fn load_proofs_for_job_ids(
    chain_id: L2ChainId,
    job_ids: &[u32],
    object_store: &dyn ObjectStore,
) -> Vec<FriProofWrapper> {
    let mut handles = Vec::with_capacity(job_ids.len());
    for job_id in job_ids {
        handles.push(FriProofWrapper::conditional_get_from_object_store(
            object_store,
            (chain_id, *job_id),
        ));
    }
    futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|x| x.unwrap())
        .collect()
}
