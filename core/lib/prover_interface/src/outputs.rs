use core::fmt;

use bellman::plonk::better_better_cs::proof::Proof as PlonkProof;
use circuit_definitions::{
    boojum::pairing::bn256::Bn256,
    circuit_definitions::aux_layer::{
        ZkSyncSnarkWrapperCircuit, ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
    },
};
use crypto_codegen::serialize_proof;
use fflonk::FflonkProof;
use serde::{Deserialize, Serialize};
use zksync_object_store::{Bucket, StoredObject, _reexports::BoxedError};
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchId, L1BatchNumber};

/// A SNARK-wrapped Airbender (ZKsync OS) proof as produced by the verifier: a bellman PLONK proof.
/// Mirrors `zkos_wrapper::SnarkWrapperProof`; the circuit type parameter is `PhantomData` in the
/// serde representation, so the verifier's proof deserializes into this type unchanged.
pub type SnarkWrapperProof = PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit>;

#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1 {
    pub(crate) inner: TypedL1BatchProofForL1,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct JsonL1BatchProofForL1(pub UntaggedTypedL1BatchProofForL1);

impl From<L1BatchProofForL1> for JsonL1BatchProofForL1 {
    fn from(value: L1BatchProofForL1) -> Self {
        Self(value.inner.into())
    }
}

impl From<JsonL1BatchProofForL1> for L1BatchProofForL1 {
    fn from(value: JsonL1BatchProofForL1) -> Self {
        Self {
            inner: value.0.into(),
        }
    }
}

/// A "final" ZK proof that can be sent to the L1 contract.
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum UntaggedTypedL1BatchProofForL1 {
    Fflonk(FflonkL1BatchProofForL1),
    Plonk(PlonkL1BatchProofForL1),
    Airbender(AirbenderL1BatchProofForL1),
}

impl From<UntaggedTypedL1BatchProofForL1> for TypedL1BatchProofForL1 {
    fn from(value: UntaggedTypedL1BatchProofForL1) -> Self {
        match value {
            UntaggedTypedL1BatchProofForL1::Fflonk(proof) => Self::Fflonk(proof),
            UntaggedTypedL1BatchProofForL1::Plonk(proof) => Self::Plonk(proof),
            UntaggedTypedL1BatchProofForL1::Airbender(proof) => Self::Airbender(proof),
        }
    }
}

impl From<TypedL1BatchProofForL1> for UntaggedTypedL1BatchProofForL1 {
    fn from(value: TypedL1BatchProofForL1) -> Self {
        match value {
            TypedL1BatchProofForL1::Fflonk(proof) => Self::Fflonk(proof),
            TypedL1BatchProofForL1::Plonk(proof) => Self::Plonk(proof),
            TypedL1BatchProofForL1::Airbender(proof) => Self::Airbender(proof),
        }
    }
}

impl L1BatchProofForL1 {
    pub fn new_fflonk(proof: FflonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Fflonk(proof),
        }
    }

    pub fn new_plonk(proof: PlonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Plonk(proof),
        }
    }

    pub fn new_airbender(proof: AirbenderL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Airbender(proof),
        }
    }

    /// Builds an Airbender (ZKsync OS) proof for L1 from the `SnarkWrapperProof` the verifier
    /// submits. The wrapper proof is a bellman PLONK proof; we flatten it into the big-endian
    /// 32-byte word layout the ZKsync OS L1 verifier expects (the same `serialize_proof` layout the
    /// Boojum Plonk path uses), so the eth_sender can submit it through `proveBatches` as
    /// verification type 2. `protocol_version` is supplied by the caller (derived from the batch
    /// number) since the wrapper proof does not carry it.
    pub fn new_airbender_from_snark_wrapper(
        snark_proof: &SnarkWrapperProof,
        protocol_version: ProtocolSemanticVersion,
    ) -> Self {
        let (_inputs, words) = serialize_proof(snark_proof);
        let mut proof = Vec::with_capacity(words.len() * 32);
        for word in words {
            let mut buf = [0u8; 32];
            word.to_big_endian(&mut buf);
            proof.extend_from_slice(&buf);
        }

        Self::new_airbender(AirbenderL1BatchProofForL1 {
            proof,
            protocol_version,
        })
    }

    pub fn inner(&self) -> TypedL1BatchProofForL1 {
        self.inner.clone()
    }
}

/// A "final" ZK proof that can be sent to the L1 contract.
#[derive(Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum TypedL1BatchProofForL1 {
    Fflonk(FflonkL1BatchProofForL1),
    Plonk(PlonkL1BatchProofForL1),
    Airbender(AirbenderL1BatchProofForL1),
}

impl L1BatchProofForL1 {
    pub fn protocol_version(&self) -> ProtocolSemanticVersion {
        match &self.inner {
            TypedL1BatchProofForL1::Fflonk(proof) => proof.protocol_version,
            TypedL1BatchProofForL1::Plonk(proof) => proof.protocol_version,
            TypedL1BatchProofForL1::Airbender(proof) => proof.protocol_version,
        }
    }

    pub fn aggregation_result_coords(&self) -> [[u8; 32]; 4] {
        match &self.inner {
            TypedL1BatchProofForL1::Fflonk(proof) => proof.aggregation_result_coords,
            TypedL1BatchProofForL1::Plonk(proof) => proof.aggregation_result_coords,
            // Airbender (ZKsync OS) proofs do not carry Boojum scheduler aggregation coords; the
            // Airbender commitment chain is computed separately and this accessor is unused for them.
            TypedL1BatchProofForL1::Airbender(_) => [[0u8; 32]; 4],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FflonkL1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: FflonkProof<Bn256, ZkSyncSnarkWrapperCircuitNoLookupCustomGate>,
    pub protocol_version: ProtocolSemanticVersion,
}

// Implementation created to allow conversion from FflonkL1BatchProofForL1(which is old L1BatchProofForL1)
// to L1BatchProofForL1 to avoid compatibility problems with serialization/deserialization
impl From<FflonkL1BatchProofForL1> for L1BatchProofForL1 {
    fn from(proof: FflonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Fflonk(proof),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PlonkL1BatchProofForL1 {
    pub aggregation_result_coords: [[u8; 32]; 4],
    pub scheduler_proof: PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit>,
    pub protocol_version: ProtocolSemanticVersion,
}

// Implementation created to allow conversion from PlonkL1BatchProofForL1(which is old L1BatchProofForL1)
// to L1BatchProofForL1 to avoid compatibility problems with serialization/deserialization
impl From<PlonkL1BatchProofForL1> for L1BatchProofForL1 {
    fn from(proof: PlonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Plonk(proof),
        }
    }
}

/// A SNARK-wrapped Airbender (ZKsync OS) proof, ready for L1 submission. The proof is opaque to
/// the server: it's the byte payload the prover produced, serialized into the word layout the
/// ZKsync OS verifier expects.
#[derive(Clone, Serialize, Deserialize)]
pub struct AirbenderL1BatchProofForL1 {
    pub proof: Vec<u8>,
    pub protocol_version: ProtocolSemanticVersion,
}

impl From<AirbenderL1BatchProofForL1> for L1BatchProofForL1 {
    fn from(proof: AirbenderL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Airbender(proof),
        }
    }
}

impl fmt::Debug for L1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L1BatchProofForL1")
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for JsonL1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JsonL1BatchProofForL1")
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PlonkL1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlonkL1BatchProofForL1")
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

impl fmt::Debug for AirbenderL1BatchProofForL1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AirbenderL1BatchProofForL1")
            .field("protocol_version", &self.protocol_version)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub enum L1BatchProofForL1Key {
    Core((L1BatchNumber, ProtocolSemanticVersion)),
    Prover((L1BatchId, ProtocolSemanticVersion)),
}

impl StoredObject for L1BatchProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = L1BatchProofForL1Key;

    fn fallback_key(key: Self::Key<'_>) -> Option<String> {
        if let L1BatchProofForL1Key::Prover((l1_batch_id, protocol_version)) = key {
            let semver_suffix = protocol_version.to_string().replace('.', "_");
            Some(format!(
                "l1_batch_proof_{batch_number}_{semver_suffix}.cbor",
                batch_number = l1_batch_id.batch_number().0
            ))
        } else {
            None
        }
    }

    fn encode_key(key: Self::Key<'_>) -> String {
        match key {
            L1BatchProofForL1Key::Core((l1_batch_number, protocol_version)) => {
                let semver_suffix = protocol_version.to_string().replace('.', "_");
                format!(
                    "l1_batch_proof_{batch_number}_{semver_suffix}.cbor",
                    batch_number = l1_batch_number.0
                )
            }
            L1BatchProofForL1Key::Prover((l1_batch_id, protocol_version)) => {
                let semver_suffix = protocol_version.to_string().replace('.', "_");
                format!(
                    "l1_batch_proof_{batch_number}_{chain_id}_{semver_suffix}.cbor",
                    batch_number = l1_batch_id.batch_number().0,
                    chain_id = l1_batch_id.chain_id()
                )
            }
        }
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).map_err(|e| {
            BoxedError::from(format!("Failed to deserialize L1BatchProofForL1: {e}"))
        })?;

        Ok(buf)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        ciborium::from_reader(&bytes[..])
            .map_err(|e| BoxedError::from(format!("Failed to deserialize L1BatchProofForL1: {e}")))
    }
}
