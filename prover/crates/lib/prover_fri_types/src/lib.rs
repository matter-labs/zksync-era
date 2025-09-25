use std::env;

pub use circuit_definitions;
use circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    circuit_definitions::{
        base_layer::ZkSyncBaseLayerProof,
        recursion_layer::{ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType},
    },
    zkevm_circuits::scheduler::{
        aux::BaseLayerCircuitType, block_header::BlockAuxilaryOutputWitness,
    },
};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{
    basic_fri_types::AggregationRound,
    protocol_version::{ProtocolSemanticVersion, VersionPatch},
    L1BatchId, L2ChainId, ProtocolVersionId,
};

pub mod keys;

pub const MAX_COMPRESSION_CIRCUITS: u8 = 5;

// THESE VALUES SHOULD BE UPDATED ON ANY PROTOCOL UPGRADE OF PROVERS
pub const PROVER_PROTOCOL_VERSION: ProtocolVersionId = ProtocolVersionId::Version29;
pub const PROVER_PROTOCOL_PATCH: VersionPatch = VersionPatch(2);
pub const PROVER_PROTOCOL_SEMANTIC_VERSION: ProtocolSemanticVersion = ProtocolSemanticVersion {
    minor: PROVER_PROTOCOL_VERSION,
    patch: PROVER_PROTOCOL_PATCH,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FriProofWrapper {
    Base(ZkSyncBaseLayerProof),
    Recursive(ZkSyncRecursionLayerProof),
}

impl StoredObject for FriProofWrapper {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = (u32, L2ChainId);

    fn fallback_key(key: Self::Key<'_>) -> Option<String> {
        Some(format!("proof_{}.bin", key.0))
    }

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("proof_{}_{}.bin", key.0, key.1)
    }

    serialize_using_bincode!();
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProverServiceDataKey {
    pub circuit_id: u8,
    pub stage: ProvingStage,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ProvingStage {
    BasicCircuits = 0,
    LeafAggregation = 1,
    NodeAggregation = 2,
    RecursionTip = 3,
    Scheduler = 4,
    Compression = 5,
    CompressionWrapper = 6,
    Snark = 7,
}

impl From<ProvingStage> for AggregationRound {
    fn from(stage: ProvingStage) -> Self {
        match stage {
            ProvingStage::BasicCircuits => AggregationRound::BasicCircuits,
            ProvingStage::LeafAggregation => AggregationRound::LeafAggregation,
            ProvingStage::NodeAggregation => AggregationRound::NodeAggregation,
            ProvingStage::RecursionTip => AggregationRound::RecursionTip,
            ProvingStage::Scheduler => AggregationRound::Scheduler,
            ProvingStage::Compression => {
                unreachable!("Compression stage is not a part of the aggregation rounds")
            }
            ProvingStage::CompressionWrapper => {
                unreachable!("Compression wrapper stage is not a part of the aggregation rounds")
            }
            ProvingStage::Snark => {
                unreachable!("Snark stage is not a part of the aggregation rounds")
            }
        }
    }
}

impl From<AggregationRound> for ProvingStage {
    fn from(round: AggregationRound) -> Self {
        match round {
            AggregationRound::BasicCircuits => ProvingStage::BasicCircuits,
            AggregationRound::LeafAggregation => ProvingStage::LeafAggregation,
            AggregationRound::NodeAggregation => ProvingStage::NodeAggregation,
            AggregationRound::RecursionTip => ProvingStage::RecursionTip,
            AggregationRound::Scheduler => ProvingStage::Scheduler,
        }
    }
}

impl ProverServiceDataKey {
    /// Returns the crypto version of the setup key.
    ///
    /// Setup key is overloaded in our system. On one hand, it is used as identifier for figuring out which type of proofs are ready.
    /// On the other hand, it is also a setup key from prover perspective.
    /// The 2 overlap on all aggregation rounds, but NodeAggregation.
    /// There's only 1 node key and that belongs to circuit 2.
    pub fn crypto_setup_key(self) -> Self {
        if self.stage == ProvingStage::NodeAggregation {
            return Self {
                circuit_id: 2,
                stage: ProvingStage::NodeAggregation,
            };
        }

        self
    }
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
    pub fn new(circuit_id: u8, round: ProvingStage) -> Self {
        Self {
            circuit_id,
            stage: round,
        }
    }

    /// Creates a new data key for recursive type - with auto selection of the aggregation round.
    pub fn new_recursive(circuit_id: u8) -> Self {
        Self {
            circuit_id,
            stage: get_round_for_recursive_circuit_type(circuit_id).into(),
        }
    }
    pub fn new_basic(circuit_id: u8) -> Self {
        Self {
            circuit_id,
            stage: ProvingStage::BasicCircuits,
        }
    }

    pub fn new_compression(circuit_id: u8) -> Self {
        Self {
            circuit_id,
            stage: ProvingStage::Compression,
        }
    }

    pub fn new_compression_wrapper(circuit_id: u8) -> Self {
        Self {
            circuit_id,
            stage: ProvingStage::CompressionWrapper,
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
        results
    }

    /// Data key for snark wrapper.
    pub fn snark() -> Self {
        Self {
            circuit_id: 1,
            stage: ProvingStage::Snark,
        }
    }

    pub fn is_base_layer(&self) -> bool {
        self.stage == ProvingStage::BasicCircuits
    }

    pub fn name(&self) -> String {
        match self.stage {
            ProvingStage::BasicCircuits => {
                format!("basic_{}", self.circuit_id)
            }
            ProvingStage::LeafAggregation => {
                format!("leaf_{}", self.circuit_id)
            }
            ProvingStage::NodeAggregation => "node".to_string(),
            ProvingStage::RecursionTip => "recursion_tip".to_string(),
            ProvingStage::Scheduler => "scheduler".to_string(),
            ProvingStage::Compression => format!("compression_{}", self.circuit_id),
            ProvingStage::CompressionWrapper => format!("compression_wrapper_{}", self.circuit_id),
            ProvingStage::Snark => "snark".to_string(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AuxOutputWitnessWrapper(pub BlockAuxilaryOutputWitness<GoldilocksField>);

impl StoredObject for AuxOutputWitnessWrapper {
    const BUCKET: Bucket = Bucket::SchedulerWitnessJobsFri;
    type Key<'a> = L1BatchId;

    fn fallback_key(key: Self::Key<'_>) -> Option<String> {
        Some(format!("aux_output_witness_{}.bin", key.batch_number().0))
    }

    fn encode_key(key: Self::Key<'_>) -> String {
        format!(
            "aux_output_witness_{}_{}.bin",
            key.batch_number().0,
            key.chain_id().as_u64()
        )
    }

    serialize_using_bincode!();
}

pub fn get_current_pod_name() -> String {
    env::var("POD_NAME").unwrap_or("UNKNOWN_POD".to_owned())
}
