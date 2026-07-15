use core::fmt;

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject};
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber};

/// A "final" Airbender proof that can be sent to the L1 contract.
#[serde_as]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchAirbenderProofForL1 {
    #[serde_as(as = "Hex")]
    pub proof: Vec<u8>,
}

impl fmt::Debug for L1BatchAirbenderProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchAirbenderProofForL1")
            .finish_non_exhaustive()
    }
}

impl StoredObject for L1BatchAirbenderProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsAirbender;
    // Keyed by `(batch number, protocol semantic version)` so FRI proofs don't collide
    // across protocol versions, matching the SNARK proof and Boojum proof keys.
    type Key<'a> = (L1BatchNumber, ProtocolSemanticVersion);

    fn encode_key(key: Self::Key<'_>) -> String {
        let (l1_batch_number, protocol_version) = key;
        let semver_suffix = protocol_version.to_string().replace('.', "_");
        format!("l1_batch_airbender_proof_{l1_batch_number}_{semver_suffix}.bin")
    }

    serialize_using_bincode!();
}

/// A SNARK-wrapped Airbender proof, suitable for L1 submission. The wrapper
/// VK is resolved at prover startup and known to the verifier out of band, so
/// it isn't stored alongside the proof.
#[serde_as]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct L1BatchAirbenderSnarkProofForL1 {
    #[serde_as(as = "Hex")]
    pub snark_proof: Vec<u8>,
}

impl fmt::Debug for L1BatchAirbenderSnarkProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchAirbenderSnarkProofForL1")
            .finish_non_exhaustive()
    }
}

impl StoredObject for L1BatchAirbenderSnarkProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsAirbender;
    // Keyed by `(batch number, protocol semantic version)` to mirror the Boojum
    // `L1BatchProofForL1` key, so the eth_sender can disambiguate proofs by the
    // verification key on L1 and avoid collisions across protocol versions.
    type Key<'a> = (L1BatchNumber, ProtocolSemanticVersion);

    fn encode_key(key: Self::Key<'_>) -> String {
        let (l1_batch_number, protocol_version) = key;
        let semver_suffix = protocol_version.to_string().replace('.', "_");
        format!("l1_batch_airbender_snark_proof_{l1_batch_number}_{semver_suffix}.bin")
    }

    serialize_using_bincode!();
}
