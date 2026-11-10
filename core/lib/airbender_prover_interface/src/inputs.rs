use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zksync_object_store::{_reexports::BoxedError, Bucket, StoredObject};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{
    block::L2BlockExecutionData, commitment::PubdataParams, L1BatchNumber, H256, U256,
};
use zksync_vm_interface::{L1BatchEnv, SystemEnv};

/// Wire-format mirror of `zksync_types::commitment::BlobHash`.
///
/// Field-for-field identical to the upstream type and to the verifier's
/// `crates/types/src/commitment::BlobHash`; defined locally because the
/// upstream struct only derives `Serialize`/`Deserialize` under `cfg(test)`.
/// Bincode-encodes as two `H256` in declaration order, matching the verifier's
/// expectation.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlobHash {
    pub commitment: H256,
    pub linear_hash: H256,
}

/// L1-settlement-bound data needed to verify the batch commitment chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitmentInput {
    /// `commitment` field of the previous (N-1) L1 batch.
    pub prev_batch_commitment: H256,
    /// `meta_parameters_hash` of the previous L1 batch.
    pub prev_meta_hash: H256,
    /// `aux_data_hash` of the previous L1 batch.
    pub prev_aux_hash: H256,
    /// One entry per EIP-4844 blob slot. Empty slots zero.
    pub blob_hashes: Vec<BlobHash>,
    /// One EIP-4844 versioned hash per blob slot.
    pub blob_versioned_hashes: Vec<H256>,
}

/// Data fed to the Airbender verifier.
///
/// `commitment_input` is `Some` when the producer can populate it (i.e., the
/// previous batch has L1 settlement metadata available); `None` for VM-only
/// consumers. The verifier requires `Some` for full proving.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AirbenderVerifierInput {
    pub vm_run_data: VMRunWitnessInputData,
    pub merkle_paths: WitnessInputMerklePaths,
    pub l2_blocks_execution_data: Vec<L2BlockExecutionData>,
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
    pub pubdata_params: PubdataParams,
    pub commitment_input: Option<CommitmentInput>,
    /// Pre-batch proofs for declared keys `merkle_paths` does not cover, in strictly ascending
    /// `leaf_hashed_key` order (the guest rejects any other order). `default` only helps JSON;
    /// the prover's bincode channel is positional.
    #[serde(default)]
    pub prestate_read_proofs: Vec<StorageReadProof>,
}

/// Read-only Merkle proof of a slot's pre-batch state against the previous batch root.
/// Hand-kept mirror of the verifier's type; both pin the same JSON literal in
/// `storage_read_proof_json_is_pinned`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageReadProof {
    /// `StorageKey::hashed_key_u256()`, as in `StorageLogMetadata::leaf_hashed_key`.
    pub leaf_hashed_key: U256,
    /// 0 = missing leaf; then `value` must be zero.
    pub leaf_enumeration_index: u64,
    pub value: [u8; 32],
    /// Leaf-to-root, empty-subtree hashes at the bottom omitted, at most 256 entries.
    pub merkle_path: Vec<[u8; 32]>,
}

/// A batch's read proofs, stored ahead of time so the request handler needs no tree access.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AirbenderReadProofs {
    pub proofs: Vec<StorageReadProof>,
}

impl StoredObject for AirbenderReadProofs {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("airbender_read_proofs_{key}.cbor")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).map_err(|e| {
            BoxedError::from(format!("Failed to serialize AirbenderReadProofs: {e}"))
        })?;
        Ok(buf)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        ciborium::from_reader(&bytes[..]).map_err(|e| {
            BoxedError::from(format!("Failed to deserialize AirbenderReadProofs: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_read_proof_json_is_pinned() {
        const GOLDEN: &str = r#"{"leaf_hashed_key":"0x1234","leaf_enumeration_index":7,"value":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,9],"merkle_path":[[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]]}"#;
        let mut value = [0; 32];
        value[31] = 9;
        let proof = StorageReadProof {
            leaf_hashed_key: U256::from(0x1234),
            leaf_enumeration_index: 7,
            value,
            merkle_path: vec![[1; 32]],
        };
        assert_eq!(serde_json::to_string(&proof).unwrap(), GOLDEN);
        assert_eq!(
            serde_json::from_str::<StorageReadProof>(GOLDEN).unwrap(),
            proof
        );
    }

    #[test]
    fn read_proofs_blob_round_trips() {
        let blob = AirbenderReadProofs {
            proofs: vec![
                StorageReadProof {
                    leaf_hashed_key: U256::MAX,
                    leaf_enumeration_index: 42,
                    value: [3; 32],
                    merkle_path: vec![[4; 32], [5; 32]],
                },
                StorageReadProof {
                    leaf_hashed_key: U256::one(),
                    leaf_enumeration_index: 0,
                    value: [0; 32],
                    merkle_path: vec![],
                },
            ],
        };
        let bytes = StoredObject::serialize(&blob).unwrap();
        assert_eq!(
            <AirbenderReadProofs as StoredObject>::deserialize(bytes).unwrap(),
            blob
        );
        assert_eq!(
            AirbenderReadProofs::encode_key(L1BatchNumber(12)),
            "airbender_read_proofs_12.cbor"
        );

        let empty = AirbenderReadProofs::default();
        let bytes = StoredObject::serialize(&empty).unwrap();
        assert_eq!(
            <AirbenderReadProofs as StoredObject>::deserialize(bytes).unwrap(),
            empty
        );
    }
}
