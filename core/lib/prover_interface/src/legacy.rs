use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zksync_object_store::{Bucket, StoredObject, _reexports::BoxedError};
use zksync_types::{
    basic_fri_types::Eip4844Blobs, protocol_version::ProtocolSemanticVersion,
    witness_block_state::WitnessStorageState, L1BatchNumber, ProtocolVersionId, U256,
};

use crate::{
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
    outputs::{
        FflonkL1BatchProofForL1, L1BatchProofForL1, PlonkL1BatchProofForL1, TypedL1BatchProofForL1,
    },
    Bincode, CBOR,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessInputDataLegacy {
    pub vm_run_data: VMRunWitnessInputDataLegacy,
    pub merkle_paths: WitnessInputMerklePaths<Bincode>,
    pub previous_batch_metadata: L1BatchMetadataHashes,
    pub eip_4844_blobs: Eip4844Blobs,
}

impl From<WitnessInputDataLegacy> for WitnessInputData<Bincode> {
    fn from(value: WitnessInputDataLegacy) -> Self {
        Self {
            vm_run_data: value.vm_run_data.into(),
            merkle_paths: value.merkle_paths,
            previous_batch_metadata: value.previous_batch_metadata,
            eip_4844_blobs: value.eip_4844_blobs,
        }
    }
}

// skip_serializing_if for field evm_emulator_code_hash doesn't work fine with bincode,
// so we are implementing custom deserialization for it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMRunWitnessInputDataLegacy {
    pub l1_batch_number: L1BatchNumber,
    pub used_bytecodes: HashMap<U256, Vec<[u8; 32]>>,
    pub initial_heap_content: Vec<(usize, U256)>,
    pub protocol_version: ProtocolVersionId,
    pub bootloader_code: Vec<[u8; 32]>,
    pub default_account_code_hash: U256,
    pub storage_refunds: Vec<u32>,
    pub pubdata_costs: Vec<i32>,
    pub witness_block_state: WitnessStorageState,
}

impl From<VMRunWitnessInputDataLegacy> for VMRunWitnessInputData<Bincode> {
    fn from(value: VMRunWitnessInputDataLegacy) -> Self {
        Self {
            l1_batch_number: value.l1_batch_number,
            used_bytecodes: value.used_bytecodes,
            initial_heap_content: value.initial_heap_content,
            protocol_version: value.protocol_version,
            bootloader_code: value.bootloader_code,
            default_account_code_hash: value.default_account_code_hash,
            evm_emulator_code_hash: None,
            storage_refunds: value.storage_refunds,
            pubdata_costs: value.pubdata_costs,
            witness_block_state: value.witness_block_state,
            _marker: std::marker::PhantomData,
        }
    }
}

impl From<VMRunWitnessInputData<Bincode>> for VMRunWitnessInputData<CBOR> {
    fn from(value: VMRunWitnessInputData<Bincode>) -> Self {
        Self {
            l1_batch_number: value.l1_batch_number,
            used_bytecodes: value.used_bytecodes,
            initial_heap_content: value.initial_heap_content,
            protocol_version: value.protocol_version,
            bootloader_code: value.bootloader_code,
            default_account_code_hash: value.default_account_code_hash,
            storage_refunds: value.storage_refunds,
            pubdata_costs: value.pubdata_costs,
            witness_block_state: value.witness_block_state,
            evm_emulator_code_hash: value.evm_emulator_code_hash,
            _marker: std::marker::PhantomData,
        }
    }
}

impl From<WitnessInputMerklePaths<Bincode>> for WitnessInputMerklePaths<CBOR> {
    fn from(value: WitnessInputMerklePaths<Bincode>) -> Self {
        Self {
            merkle_paths: value.merkle_paths,
            next_enumeration_index: value.next_enumeration_index,
            _marker: std::marker::PhantomData,
        }
    }
}

impl From<WitnessInputData<Bincode>> for WitnessInputData<CBOR> {
    fn from(value: WitnessInputData<Bincode>) -> Self {
        Self {
            vm_run_data: value.vm_run_data.into(),
            merkle_paths: value.merkle_paths.into(),
            previous_batch_metadata: value.previous_batch_metadata,
            eip_4844_blobs: value.eip_4844_blobs,
        }
    }
}

impl From<L1BatchProofForL1<Bincode>> for L1BatchProofForL1<CBOR> {
    fn from(value: L1BatchProofForL1<Bincode>) -> Self {
        match value.inner {
            TypedL1BatchProofForL1::Fflonk(proof) => Self {
                inner: TypedL1BatchProofForL1::Fflonk(proof),
                _marker: std::marker::PhantomData,
            },
            TypedL1BatchProofForL1::Plonk(proof) => Self {
                inner: TypedL1BatchProofForL1::Plonk(proof),
                _marker: std::marker::PhantomData,
            },
        }
    }
}

impl StoredObject for L1BatchProofForL1<Bincode> {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = (L1BatchNumber, ProtocolSemanticVersion);

    fn encode_key(key: Self::Key<'_>) -> String {
        let (l1_batch_number, protocol_version) = key;
        let semver_suffix = protocol_version.to_string().replace('.', "_");
        format!("l1_batch_proof_{l1_batch_number}_{semver_suffix}.bin")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        match &self.inner {
            TypedL1BatchProofForL1::Fflonk(proof) => {
                zksync_object_store::bincode::serialize(proof).map_err(From::from)
            }
            TypedL1BatchProofForL1::Plonk(proof) => {
                zksync_object_store::bincode::serialize(proof).map_err(From::from)
            }
        }
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        match zksync_object_store::bincode::deserialize::<PlonkL1BatchProofForL1>(&bytes) {
            Ok(proof) => Ok(proof.into()),
            Err(_) => zksync_object_store::bincode::deserialize::<FflonkL1BatchProofForL1>(&bytes)
                .map(Into::into)
                .map_err(Into::into),
        }
    }
}

impl StoredObject for VMRunWitnessInputData<Bincode> {
    const BUCKET: Bucket = Bucket::WitnessInput;

    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("vm_run_data_{key}.bin")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        zksync_object_store::bincode::serialize(self).map_err(Into::into)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        zksync_object_store::bincode::deserialize::<VMRunWitnessInputData<Bincode>>(&bytes).or_else(
            |_| {
                zksync_object_store::bincode::deserialize::<VMRunWitnessInputDataLegacy>(&bytes)
                    .map(Into::into)
                    .map_err(Into::into)
            },
        )
    }
}

impl StoredObject for WitnessInputData<Bincode> {
    const BUCKET: Bucket = Bucket::WitnessInput;

    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("witness_input_data_{key}.bin")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        zksync_object_store::bincode::serialize(self).map_err(Into::into)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        zksync_object_store::bincode::deserialize::<WitnessInputData<Bincode>>(&bytes).or_else(
            |_| {
                zksync_object_store::bincode::deserialize::<WitnessInputDataLegacy>(&bytes)
                    .map(Into::into)
                    .map_err(Into::into)
            },
        )
    }
}

impl StoredObject for WitnessInputMerklePaths<Bincode> {
    const BUCKET: Bucket = Bucket::WitnessInput;

    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("merkel_tree_paths_{key}.bin")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        zksync_object_store::bincode::serialize(self).map_err(Into::into)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        zksync_object_store::bincode::deserialize::<WitnessInputMerklePaths<Bincode>>(&bytes)
            .or_else(|_| {
                zksync_object_store::bincode::deserialize::<WitnessInputMerklePaths<Bincode>>(
                    &bytes,
                )
                .map(Into::into)
                .map_err(Into::into)
            })
    }
}
