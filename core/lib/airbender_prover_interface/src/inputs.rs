use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{block::L2BlockExecutionData, commitment::PubdataParams, H256, U256};
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

/// A Merkle proof of a single storage slot's pre-batch state against
/// `old_root_hash`, for a slot the committed `merkle_paths` omits (a read inside
/// a reverted frame, or a reverted initial write). Wire-format mirror of the
/// verifier's `ReadProof`; field order is load-bearing for bincode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadProof {
    /// Hashed storage key (little-endian `U256`, matching `TreeEntry`).
    pub hashed_key: U256,
    /// Pre-state value; zero for an empty/absent slot.
    pub value: H256,
    /// Enumeration index; 0 for an empty/initial slot.
    pub enumeration_index: u64,
    /// Merkle path from the leaf to `old_root_hash`.
    pub merkle_path: Vec<H256>,
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
    /// Proofs (vs `old_root_hash`) for view-domain slots absent from
    /// `merkle_paths`. Empty when every read the VM makes is committed.
    #[serde(default)]
    pub read_proofs: Vec<ReadProof>,
}

#[cfg(test)]
mod tests {
    use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;
    use zksync_merkle_tree::{MerkleTree, PatchSet, TreeEntry, TreeEntryWithProof};
    use zksync_types::{H256, U256};

    use super::ReadProof;

    #[test]
    fn read_proof_roundtrips_a_real_tree_proof() {
        // Build a one-leaf tree; capture root + proofs for a present and an absent key.
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        let present = U256::from(0x1111u64);
        let absent = U256::from(0x2222u64);
        let value = H256::from_low_u64_be(0x42);
        let root = tree
            .extend(vec![TreeEntry::new(present, 1, value)])
            .unwrap()
            .root_hash;
        let entries = tree.entries_with_proofs(0, &[present, absent]).unwrap();

        // Map each tree proof into our ReadProof exactly as the handler does.
        let to_read_proof = |key: U256, e: &TreeEntryWithProof| ReadProof {
            hashed_key: key,
            value: e.base.value,
            enumeration_index: e.base.leaf_index,
            merkle_path: e.merkle_path.clone(),
        };
        let present_rp = to_read_proof(present, &entries[0]);
        let absent_rp = to_read_proof(absent, &entries[1]);

        // Reconstruct the tree proof from our ReadProof and confirm it still folds to root.
        for rp in [&present_rp, &absent_rp] {
            let reconstructed = TreeEntryWithProof {
                base: TreeEntry::new(rp.hashed_key, rp.enumeration_index, rp.value),
                merkle_path: rp.merkle_path.clone(),
            };
            reconstructed
                .verify(&Blake2Hasher, root)
                .expect("ReadProof must reconstruct a valid proof");
        }

        // Sanity: the present slot carries the value/index; the absent slot is empty (index 0, value 0).
        assert_eq!(present_rp.value, value);
        assert_eq!(present_rp.enumeration_index, 1);
        assert_eq!(absent_rp.enumeration_index, 0);
        assert!(absent_rp.value.is_zero());
    }
}
