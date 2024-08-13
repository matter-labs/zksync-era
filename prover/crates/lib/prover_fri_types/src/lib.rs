use std::env;

pub use circuit_definitions;
use circuit_definitions::{
    boojum::{cs::implementations::witness::WitnessVec, field::goldilocks::GoldilocksField},
    circuit_definitions::{
        base_layer::{ZkSyncBaseLayerCircuit, ZkSyncBaseLayerProof},
        recursion_layer::{
            ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType, ZkSyncRecursiveLayerCircuit,
        },
    },
    encodings::memory_query::MemoryQueueStateWitnesses,
    zkevm_circuits::scheduler::{
        aux::BaseLayerCircuitType, block_header::BlockAuxilaryOutputWitness,
    },
};
use keys::RamPermutationQueueWitnessKey;
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{
    basic_fri_types::AggregationRound,
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    L1BatchNumber, ProtocolVersionId,
};

use crate::keys::FriCircuitKey;

pub mod keys;
pub mod queue;

// THESE VALUES SHOULD BE UPDATED ON ANY PROTOCOL UPGRADE OF PROVERS
pub const PROVER_PROTOCOL_VERSION: ProtocolVersionId = ProtocolVersionId::Version24;
pub const PROVER_PROTOCOL_PATCH: VersionPatch = VersionPatch(2);
pub const PROVER_PROTOCOL_SEMANTIC_VERSION: ProtocolSemanticVersion = ProtocolSemanticVersion {
    minor: PROVER_PROTOCOL_VERSION,
    patch: PROVER_PROTOCOL_PATCH,
};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct CircuitAuxData {
    pub circuit_subsequence_number: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CircuitWrapper {
    Base(ZkSyncBaseLayerCircuit),
    Recursive(ZkSyncRecursiveLayerCircuit),
    BaseWithAuxData((ZkSyncBaseLayerCircuit, CircuitAuxData)),
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

#[derive(Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct WitnessVectorArtifacts {
    pub witness_vector: WitnessVec<GoldilocksField>,
    pub prover_job: ProverJob,
}

impl WitnessVectorArtifacts {
    pub fn new(witness_vector: WitnessVec<GoldilocksField>, prover_job: ProverJob) -> Self {
        Self {
            witness_vector,
            prover_job,
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ProverJob {
    pub block_number: L1BatchNumber,
    pub job_id: u32,
    pub circuit_wrapper: CircuitWrapper,
    pub setup_data_key: ProverServiceDataKey,
}

impl ProverJob {
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

#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProverServiceDataKey {
    pub circuit_id: u8,
    pub round: AggregationRound,
}

fn get_round_for_recursive_circuit_type(circuit_type: u8) -> AggregationRound {
    match circuit_type {
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8 => {
            AggregationRound::Scheduler
        }
        circuit_type if circuit_type == ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8 => {
            AggregationRound::NodeAggregation
        }
        circuit_type
            if circuit_type == ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8 =>
        {
            AggregationRound::RecursionTip
        }
        _ => AggregationRound::LeafAggregation,
    }
}

impl ProverServiceDataKey {
    pub fn new(circuit_id: u8, round: AggregationRound) -> Self {
        Self { circuit_id, round }
    }

    /// Creates a new data key for recursive type - with auto selection of the aggregation round.
    pub fn new_recursive(circuit_id: u8) -> Self {
        Self {
            circuit_id,
            round: get_round_for_recursive_circuit_type(circuit_id),
        }
    }
    pub fn new_basic(circuit_id: u8) -> Self {
        Self {
            circuit_id,
            round: AggregationRound::BasicCircuits,
        }
    }

    pub fn all_boojum() -> Vec<ProverServiceDataKey> {
        let mut results = vec![];
        for numeric_circuit in BaseLayerCircuitType::as_iter_u8() {
            results.push(ProverServiceDataKey::new_basic(numeric_circuit))
        }
        for numeric_circuit in ZkSyncRecursionLayerStorageType::as_iter_u8() {
            results.push(ProverServiceDataKey::new_recursive(numeric_circuit))
        }

        // Don't include snark, as it uses the old proving system.

        results
    }

    /// Data key for snark wrapper.
    pub fn snark() -> Self {
        Self {
            circuit_id: 1,
            round: AggregationRound::Scheduler,
        }
    }

    pub fn is_base_layer(&self) -> bool {
        self.round == AggregationRound::BasicCircuits
    }

    pub fn name(&self) -> String {
        match self.round {
            AggregationRound::BasicCircuits => {
                format!("basic_{}", self.circuit_id)
            }
            AggregationRound::LeafAggregation => {
                format!("leaf_{}", self.circuit_id)
            }
            AggregationRound::NodeAggregation => "node".to_string(),
            AggregationRound::RecursionTip => "recursion_tip".to_string(),
            AggregationRound::Scheduler => "scheduler".to_string(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AuxOutputWitnessWrapper(pub BlockAuxilaryOutputWitness<GoldilocksField>);

impl StoredObject for AuxOutputWitnessWrapper {
    const BUCKET: Bucket = Bucket::SchedulerWitnessJobsFri;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("aux_output_witness_{key}.bin")
    }

    serialize_using_bincode!();
}

pub fn get_current_pod_name() -> String {
    env::var("POD_NAME").unwrap_or("UNKNOWN_POD".to_owned())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RamPermutationQueueWitness {
    pub is_sorted: bool,
    pub witness: MemoryQueueStateWitnesses<GoldilocksField>,
}

impl StoredObject for RamPermutationQueueWitness {
    const BUCKET: Bucket = Bucket::RamPermutationCircuitQueueWitness;
    type Key<'a> = RamPermutationQueueWitnessKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        let RamPermutationQueueWitnessKey {
            block_number,
            circuit_subsequence_number,
            is_sorted,
        } = key;
        format!(
            "queue_witness_{block_number}_{circuit_subsequence_number}_{}.bin",
            is_sorted as u64
        )
    }

    serialize_using_bincode!();
}
