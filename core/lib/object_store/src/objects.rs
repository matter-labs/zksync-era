//! Stored objects.

use zksync_types::aggregated_operations::L1BatchProofForL1;
use zksync_types::{
    proofs::{AggregationRound, PrepareBasicCircuitsJob},
    storage::witness_block_state::WitnessBlockState,
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::bn256::Bn256,
        encodings::{recursion_request::RecursionRequest, QueueSimulator},
        witness::full_block_artifact::{BlockBasicCircuits, BlockBasicCircuitsPublicInputs},
        witness::oracle::VmWitnessOracle,
        LeafAggregationOutputDataWitness, NodeAggregationOutputDataWitness,
        SchedulerCircuitInstanceWitness,
    },
    L1BatchNumber,
};

use crate::raw::{BoxedError, Bucket, ObjectStore, ObjectStoreError};

/// Object that can be stored in an [`ObjectStore`].
pub trait StoredObject: Sized {
    /// Bucket in which values are stored.
    const BUCKET: Bucket;
    /// Logical unique key for the object. The lifetime param allows defining keys
    /// that borrow data; see [`CircuitKey`] for an example.
    type Key<'a>: Copy;

    /// Encodes the object key to a string.
    fn encode_key(key: Self::Key<'_>) -> String;

    /// Serializes a value to a blob.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    fn serialize(&self) -> Result<Vec<u8>, BoxedError>;

    /// Deserializes a value from the blob.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError>;
}

/// Derives [`StoredObject::serialize()`] and [`StoredObject::deserialize()`] using
/// the `bincode` (de)serializer. Should be used in `impl StoredObject` blocks.
#[macro_export]
macro_rules! serialize_using_bincode {
    () => {
        fn serialize(
            &self,
        ) -> std::result::Result<std::vec::Vec<u8>, $crate::_reexports::BoxedError> {
            $crate::bincode::serialize(self).map_err(std::convert::From::from)
        }

        fn deserialize(
            bytes: std::vec::Vec<u8>,
        ) -> std::result::Result<Self, $crate::_reexports::BoxedError> {
            $crate::bincode::deserialize(&bytes).map_err(std::convert::From::from)
        }
    };
}

impl StoredObject for WitnessBlockState {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("witness_block_state_for_l1_batch_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for PrepareBasicCircuitsJob {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("merkel_tree_paths_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for BlockBasicCircuits<Bn256> {
    const BUCKET: Bucket = Bucket::LeafAggregationWitnessJobs;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("basic_circuits_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for BlockBasicCircuitsPublicInputs<Bn256> {
    const BUCKET: Bucket = Bucket::LeafAggregationWitnessJobs;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("basic_circuits_inputs_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for SchedulerCircuitInstanceWitness<Bn256> {
    const BUCKET: Bucket = Bucket::SchedulerWitnessJobs;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("scheduler_witness_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for NodeAggregationOutputDataWitness<Bn256> {
    const BUCKET: Bucket = Bucket::SchedulerWitnessJobs;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("final_node_aggregations_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for Vec<LeafAggregationOutputDataWitness<Bn256>> {
    const BUCKET: Bucket = Bucket::NodeAggregationWitnessJobs;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("aggregation_outputs_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for Vec<QueueSimulator<Bn256, RecursionRequest<Bn256>, 2, 2>> {
    const BUCKET: Bucket = Bucket::NodeAggregationWitnessJobs;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("leaf_layer_subqueues_{key}.bin")
    }

    serialize_using_bincode!();
}

/// Storage key for a [AggregationWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct AggregationsKey {
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
    pub depth: u16,
}

/// Storage key for a [ClosedFormInputWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct ClosedFormInputKey {
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
}

/// Storage key for a [`CircuitWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct FriCircuitKey {
    pub block_number: L1BatchNumber,
    pub sequence_number: usize,
    pub circuit_id: u8,
    pub aggregation_round: AggregationRound,
    pub depth: u16,
}

/// Storage key for a [`ZkSyncCircuit`].
#[derive(Debug, Clone, Copy)]
pub struct CircuitKey<'a> {
    pub block_number: L1BatchNumber,
    pub sequence_number: usize,
    pub circuit_type: &'a str,
    pub aggregation_round: AggregationRound,
}

impl StoredObject for ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>> {
    const BUCKET: Bucket = Bucket::ProverJobs;
    type Key<'a> = CircuitKey<'a>;

    fn encode_key(key: Self::Key<'_>) -> String {
        let CircuitKey {
            block_number,
            sequence_number,
            circuit_type,
            aggregation_round,
        } = key;
        format!("{block_number}_{sequence_number}_{circuit_type}_{aggregation_round:?}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for L1BatchProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_proof_{key}.bin")
    }

    serialize_using_bincode!();
}

impl dyn ObjectStore + '_ {
    /// Fetches the value for the given key if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error if an object with the `key` does not exist, cannot be accessed,
    /// or cannot be deserialized.
    pub async fn get<V: StoredObject>(&self, key: V::Key<'_>) -> Result<V, ObjectStoreError> {
        let key = V::encode_key(key);
        let bytes = self.get_raw(V::BUCKET, &key).await?;
        V::deserialize(bytes).map_err(ObjectStoreError::Serialization)
    }

    /// Stores the value associating it with the key. If the key already exists,
    /// the value is replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or the insertion / replacement operation fails.
    pub async fn put<V: StoredObject>(
        &self,
        key: V::Key<'_>,
        value: &V,
    ) -> Result<String, ObjectStoreError> {
        let key = V::encode_key(key);
        let bytes = value.serialize().map_err(ObjectStoreError::Serialization)?;
        self.put_raw(V::BUCKET, &key, bytes).await?;
        Ok(key)
    }
}
