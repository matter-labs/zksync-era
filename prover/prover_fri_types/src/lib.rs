use std::env;

// Re-exporting module circuit_definitions
pub use circuit_definitions;
use circuit_definitions::{
    boojum::{cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField},
    circuit_definitions::{
        base_layer::{ZkSyncBaseLayerCircuit, ZkSyncBaseLayerProof, ZkSyncBaseProof},
        eip4844::EIP4844Circuit,
        recursion_layer::{
            ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType, ZkSyncRecursiveLayerCircuit,
        },
    },
    zkevm_circuits::scheduler::{
        aux::BaseLayerCircuitType, block_header::BlockAuxilaryOutputWitness,
    },
};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{basic_fri_types::AggregationRound, L1BatchNumber};

use crate::keys::FriCircuitKey;

// Module declarations
pub mod keys;
pub mod queue;

// Constants
pub const EIP_4844_CIRCUIT_ID: u8 = 255;

// Enum representing different circuit types
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CircuitWrapper {
    Base(ZkSyncBaseLayerCircuit),
    Recursive(ZkSyncRecursiveLayerCircuit),
    Eip4844(EIP4844Circuit),
}

// Implementing `StoredObject` trait for `CircuitWrapper`
impl StoredObject for CircuitWrapper {
    const BUCKET: Bucket = Bucket::ProverJobsFri;
    type Key<'a> = FriCircuitKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        // Encoding key into a string
        let FriCircuitKey {
            block_number,
            sequence_number,
            circuit_id,
            aggregation_round,
            depth,
        } = key;
        format!("{block_number}_{sequence_number}_{circuit_id}_{aggregation_round:?}_{depth}.bin")
    }

    // Using bincode for serialization
    serialize_using_bincode!();
}

// Enum representing different types of FRI proofs
#[derive(serde::Serialize, serde::Deserialize)]
pub enum FriProofWrapper {
    Base(ZkSyncBaseLayerProof),
    Recursive(ZkSyncRecursionLayerProof),
    Eip4844(ZkSyncBaseProof),
}

// Implementing `StoredObject` trait for `FriProofWrapper`
impl StoredObject for FriProofWrapper {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = u32;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("proof_{key}.bin")
    }

    // Using bincode for serialization
    serialize_using_bincode!();
}

// Struct representing artifacts related to witness vectors
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct WitnessVectorArtifacts {
    pub witness_vector: WitnessVec<GoldilocksField>,
    pub prover_job: ProverJob,
}

// Implementing methods for `WitnessVectorArtifacts`
impl WitnessVectorArtifacts {
    // Constructor method
    pub fn new(witness_vector: WitnessVec<GoldilocksField>, prover_job: ProverJob) -> Self {
        Self {
            witness_vector,
            prover_job,
        }
    }
}

// Struct representing a prover job
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ProverJob {
    pub block_number: L1BatchNumber,
    pub job_id: u32,
    pub circuit_wrapper: CircuitWrapper,
    pub setup_data_key: ProverServiceDataKey,
}

// Implementing methods for `ProverJob`
impl ProverJob {
    // Constructor method
    pub fn new(
        block_number: L1BatchNumber,
        job_id: u32,
        circuit_wrapper: CircuitWrapper,
        setup_data_key: ProverServiceDataKey,
    ) -> Self {
        Self {
            block_number,
            job_id,
            circuit_wrapper,
            setup_data_key,
        }
    }
}

// Struct representing a key for service data
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProverServiceDataKey {
    pub circuit_id: u8,
    pub round: AggregationRound,
}

// Function to get the aggregation round for a recursive circuit type
fn get_round_for_recursive_circuit_type(circuit_type: u8) -> AggregationRound {
    // Implementation details skipped for brevity
    match circuit_type {
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8 => {
            AggregationRound::Scheduler
        }
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8 => {
            AggregationRound::NodeAggregation
        }
        _ => AggregationRound::LeafAggregation,
    }
}

// Implementing methods for `ProverServiceDataKey`
impl ProverServiceDataKey {
    // Constructor method
    pub fn new(circuit_id: u8, round: AggregationRound) -> Self {
        Self { circuit_id, round }
    }

    // Method to create a new data key for a recursive type
    pub fn new_recursive(circuit_id: u8) -> Self {
        // Implementation details skipped for brevity
         Self {
            circuit_id,
            round: get_round_for_recursive_circuit_type(circuit_id),
        }
    }
    // Method to create a new data key for a basic type
    pub fn new_basic(circuit_id: u8) -> Self {
        // Implementation details skipped for brevity
        Self {
            circuit_id,
            round: AggregationRound::BasicCircuits,
        }
    }

    // Method to get keys for all Boojum circuits
    pub fn all_boojum() -> Vec<ProverServiceDataKey> {
        // Implementation details skipped for brevity
        let mut results = vec![];
        for numeric_circuit in
            BaseLayerCircuitType::VM as u8..=BaseLayerCircuitType::L1MessagesHasher as u8
        {
            results.push(ProverServiceDataKey::new_basic(numeric_circuit))
        }
        for numeric_circuit in ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8
            ..=ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8
        {
            results.push(ProverServiceDataKey::new_recursive(numeric_circuit))
        }
        results.push(ProverServiceDataKey::eip4844());
        // Don't include snark, as it uses the old proving system.

        results
    }

    // Method to get the data key for a snark wrapper
    pub fn snark() -> Self {
        // Implementation details skipped for brevity
         Self {
            circuit_id: 1,
            round: AggregationRound::Scheduler,
        }
    }

    // Method to get the data key for EIP-4844 circuit
    pub fn eip4844() -> Self {
        // Implementation details skipped for brevity
         Self {
            circuit_id: EIP_4844_CIRCUIT_ID,
            round: AggregationRound::BasicCircuits,
        }
    }
    // Method to check if the key represents an EIP-4844 circuit
    pub fn is_eip4844(&self) -> bool {
        // Implementation details skipped for brevity
    }

    // Method to check if the key represents a base layer circuit
    pub fn is_base_layer(&self) -> bool {
        // Implementation details skipped for brevity
        self.round == AggregationRound::BasicCircuits && !self.is_eip4844()
    }

    // Method to get the name corresponding to the key
    pub fn name(&self) -> String {
        // Implementation details skipped for brevity
        if self.is_eip4844() {
            return "eip4844".to_string();
        }
        match self.round {
            AggregationRound::BasicCircuits => {
                format!("basic_{}", self.circuit_id)
            }
            AggregationRound::LeafAggregation => {
                format!("leaf_{}", self.circuit_id)
            }
            AggregationRound::NodeAggregation => "node".to_string(),
            AggregationRound::Scheduler => "scheduler".to_string(),
        }
    }
}

// Struct representing auxiliary output witness wrapper
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AuxOutputWitnessWrapper(pub BlockAuxilaryOutputWitness<GoldilocksField>);

// Implementing `StoredObject` trait for `AuxOutputWitnessWrapper`
impl StoredObject for AuxOutputWitnessWrapper {
    const BUCKET: Bucket = Bucket::SchedulerWitnessJobsFri;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("aux_output_witness_{key}.bin")
    }

    // Using bincode for serialization
    serialize_using_bincode!();
}

// Function to get the current pod name
pub fn get_current_pod_name() -> String {
    env::var("POD_NAME").unwrap_or("UNKNOWN_POD".to_owned())
}
