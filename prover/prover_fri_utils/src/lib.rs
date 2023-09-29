use zksync_object_store::FriCircuitKey;
use zksync_object_store::StoredObject;
use zksync_object_store::Bucket;
use zksync_object_store::serialize_using_bincode;

use circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;
use circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerProof;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;

use circuit_definitions::circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type;
use circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;

use circuit_definitions::ZkSyncDefaultRoundFunction;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursiveLayerCircuit;
use circuit_definitions::boojum::field::goldilocks::GoldilocksField;

use circuit_definitions::circuit_definitions::base_layer::{
    ZkSyncBaseLayerCircuit,
};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum CircuitWrapper {
    Base(
        ZkSyncBaseLayerCircuit<
            GoldilocksField,
            VmWitnessOracle<GoldilocksField>,
            ZkSyncDefaultRoundFunction,
        >,
    ),
    Recursive(ZkSyncRecursiveLayerCircuit),
}

impl StoredObject for CircuitWrapper {
    const BUCKET: Bucket = Bucket::ProverJobsFri;
    type Key<'a> = FriCircuitKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        let FriCircuitKey {
            block_number,
            sequence_number,
            circuit_id,
            aggregation_round,
            depth,
        } = key;
        format!("{block_number}_{sequence_number}_{circuit_id}_{aggregation_round:?}_{depth}.bin")
    }

    serialize_using_bincode!();
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum FriProofWrapper {
    Base(ZkSyncBaseLayerProof),
    Recursive(ZkSyncRecursionLayerProof),
}

impl StoredObject for FriProofWrapper {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = u32;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("proof_{key}.bin")
    }

    serialize_using_bincode!();
}

pub fn get_recursive_layer_circuit_id_for_base_layer(base_layer_circuit_id: u8) -> u8 {
    let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
        BaseLayerCircuitType::from_numeric_value(base_layer_circuit_id),
    );
    recursive_circuit_type as u8
}


pub fn get_base_layer_circuit_id_for_recursive_layer(recursive_layer_circuit_id: u8) -> u8 {
    recursive_layer_circuit_id - ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8
}
