use core::fmt;

use circuit_sequencer_api_1_5_0::proof::FinalProof;
use fflonk::FflonkSnarkVerifierCircuitProof;
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{protocol_version::ProtocolSemanticVersion, tee_types::TeeType, L1BatchNumber};

/// A "final" ZK proof that can be sent to the L1 contract.
#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: SchedulerProof,
    pub protocol_version: ProtocolSemanticVersion,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FflonkL1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: FflonkSnarkVerifierCircuitProof,
    pub protocol_version: ProtocolSemanticVersion,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PplonkL1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: FinalProof,
    pub protocol_version: ProtocolSemanticVersion,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum SchedulerProof {
    Fflonk(Box<FflonkSnarkVerifierCircuitProof>),
    Pplonk(Box<FinalProof>),
}

impl From<L1BatchProofForL1> for FflonkL1BatchProofForL1 {
    fn from(proof: L1BatchProofForL1) -> Self {
        match proof.scheduler_proof {
            SchedulerProof::Fflonk(scheduler_proof) => FflonkL1BatchProofForL1 {
                aggregation_result_coords: proof.aggregation_result_coords,
                scheduler_proof: *scheduler_proof,
                protocol_version: proof.protocol_version,
            },
            _ => panic!("Invalid proof type, expected FFLONK proof"),
        }
    }
}

impl From<L1BatchProofForL1> for PplonkL1BatchProofForL1 {
    fn from(proof: L1BatchProofForL1) -> Self {
        match proof.scheduler_proof {
            SchedulerProof::Pplonk(scheduler_proof) => PplonkL1BatchProofForL1 {
                aggregation_result_coords: proof.aggregation_result_coords,
                scheduler_proof: *scheduler_proof,
                protocol_version: proof.protocol_version,
            },
            _ => panic!("Invalid proof type, expected PPLONK proof"),
        }
    }
}

/// A "final" TEE proof that can be sent to the L1 contract.
#[serde_as]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchTeeProofForL1 {
    // signature generated within the TEE enclave, using the privkey corresponding to the pubkey
    #[serde_as(as = "Hex")]
    pub signature: Vec<u8>,
    // pubkey used for signature verification; each key pair is attested by the TEE attestation
    // stored in the db
    #[serde_as(as = "Hex")]
    pub pubkey: Vec<u8>,
    // data that was signed
    #[serde_as(as = "Hex")]
    pub proof: Vec<u8>,
    // type of TEE used for attestation
    pub tee_type: TeeType,
}

impl fmt::Debug for L1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchProofForL1")
            .field("aggregation_result_coords", &self.aggregation_result_coords)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PplonkL1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PplonkL1BatchProofForL1")
            .field("aggregation_result_coords", &self.aggregation_result_coords)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for FflonkL1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FflonkL1BatchProofForL1")
            .field("aggregation_result_coords", &self.aggregation_result_coords)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for L1BatchTeeProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchTeeProofForL1")
            .finish_non_exhaustive()
    }
}

impl StoredObject for L1BatchProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = (L1BatchNumber, ProtocolSemanticVersion);

    fn encode_key(key: Self::Key<'_>) -> String {
        let (l1_batch_number, protocol_version) = key;
        let semver_suffix = protocol_version.to_string().replace('.', "_");
        format!("l1_batch_proof_{l1_batch_number}_{semver_suffix}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for L1BatchTeeProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsTee;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_tee_proof_{key}.bin")
    }

    serialize_using_bincode!();
}
