use core::fmt;

use bellman::plonk::better_better_cs::proof::Proof as PlonkProof;
use circuit_definitions::{
    boojum::pairing::bn256::Bn256,
    circuit_definitions::aux_layer::{
        ZkSyncSnarkWrapperCircuit, ZkSyncSnarkWrapperCircuitNoLookupCustomGate,
    },
};
use fflonk::FflonkProof;
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use zksync_object_store::{serialize_using_bincode, Bucket, StoredObject, _reexports::BoxedError};
use zksync_types::{protocol_version::ProtocolSemanticVersion, tee_types::TeeType, L1BatchNumber};

use crate::{FormatMarker, CBOR};

#[derive(Clone, Serialize, Deserialize)]
pub struct L1BatchProofForL1<FM: FormatMarker = CBOR> {
    pub(crate) inner: TypedL1BatchProofForL1,
    #[serde(skip)]
    pub(crate) _marker: std::marker::PhantomData<FM>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct JsonL1BatchProofForL1(pub UntaggedTypedL1BatchProofForL1);

impl<FM: FormatMarker> From<L1BatchProofForL1<FM>> for JsonL1BatchProofForL1 {
    fn from(value: L1BatchProofForL1<FM>) -> Self {
        Self(value.inner.into())
    }
}

impl<FM: FormatMarker> From<JsonL1BatchProofForL1> for L1BatchProofForL1<FM> {
    fn from(value: JsonL1BatchProofForL1) -> Self {
        Self {
            inner: value.0.into(),
            _marker: std::marker::PhantomData,
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
}

impl From<UntaggedTypedL1BatchProofForL1> for TypedL1BatchProofForL1 {
    fn from(value: UntaggedTypedL1BatchProofForL1) -> Self {
        match value {
            UntaggedTypedL1BatchProofForL1::Fflonk(proof) => Self::Fflonk(proof),
            UntaggedTypedL1BatchProofForL1::Plonk(proof) => Self::Plonk(proof),
        }
    }
}

impl From<TypedL1BatchProofForL1> for UntaggedTypedL1BatchProofForL1 {
    fn from(value: TypedL1BatchProofForL1) -> Self {
        match value {
            TypedL1BatchProofForL1::Fflonk(proof) => Self::Fflonk(proof),
            TypedL1BatchProofForL1::Plonk(proof) => Self::Plonk(proof),
        }
    }
}

impl<FM: FormatMarker> L1BatchProofForL1<FM> {
    pub fn new_fflonk(proof: FflonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Fflonk(proof),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn new_plonk(proof: PlonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Plonk(proof),
            _marker: std::marker::PhantomData,
        }
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
}

impl L1BatchProofForL1 {
    pub fn protocol_version(&self) -> ProtocolSemanticVersion {
        match &self.inner {
            TypedL1BatchProofForL1::Fflonk(proof) => proof.protocol_version,
            TypedL1BatchProofForL1::Plonk(proof) => proof.protocol_version,
        }
    }

    pub fn aggregation_result_coords(&self) -> [[u8; 32]; 4] {
        match &self.inner {
            TypedL1BatchProofForL1::Fflonk(proof) => proof.aggregation_result_coords,
            TypedL1BatchProofForL1::Plonk(proof) => proof.aggregation_result_coords,
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
impl<FM: FormatMarker> From<FflonkL1BatchProofForL1> for L1BatchProofForL1<FM> {
    fn from(proof: FflonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Fflonk(proof),
            _marker: std::marker::PhantomData,
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
impl<FM: FormatMarker> From<PlonkL1BatchProofForL1> for L1BatchProofForL1<FM> {
    fn from(proof: PlonkL1BatchProofForL1) -> Self {
        Self {
            inner: TypedL1BatchProofForL1::Plonk(proof),
            _marker: std::marker::PhantomData,
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
        format!("l1_batch_proof_{l1_batch_number}_{semver_suffix}.cbor")
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

impl StoredObject for L1BatchTeeProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsTee;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_tee_proof_{key}.bin")
    }

    serialize_using_bincode!();
}
