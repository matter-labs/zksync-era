use core::fmt;

use circuit_sequencer_api_1_5_0::proof::FinalProof;
use fflonk::FflonkSnarkVerifierCircuitProof;
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{protocol_version::ProtocolSemanticVersion, tee_types::TeeType, L1BatchNumber};

/// A "final" ZK proof that can be sent to the L1 contract.
#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1<T: FinalProofExt> {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: T,
    pub protocol_version: ProtocolSemanticVersion,
}

pub trait FinalProofExt: Serialize + Deserialize {}

impl FinalProofExt for FinalProof {}
impl FinalProofExt for FflonkSnarkVerifierCircuitProof {}

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

impl<T: FinalProofExt> fmt::Debug for L1BatchProofForL1<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchProofForL1")
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

impl<T: FinalProofExt> StoredObject for L1BatchProofForL1<T> {
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
