//! Property tests for the Merkle tree.

use std::{
    collections::{HashMap, HashSet},
    num::NonZeroI8,
    ops,
};

use proptest::{prelude::*, sample::Index};
use zksync_basic_types::H256;
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

use super::naive_hash_tree;
use crate::{
    types::Leaf, BatchOutput, BatchTreeProof, DefaultTreeParams, MerkleTree, PatchSet, TreeEntry,
    TreeOperation, TreeParams,
};

const MAX_ENTRIES: usize = 100;

fn uniform_hash() -> impl Strategy<Value = H256> {
    proptest::array::uniform32(proptest::num::u8::ANY)
        .prop_map(H256)
        .prop_filter("guard", |hash| {
            *hash != H256::zero() && *hash != H256::repeat_byte(0xff)
        })
}

fn gen_writes(size: ops::RangeInclusive<usize>) -> impl Strategy<Value = Vec<TreeEntry>> {
    proptest::collection::hash_map(uniform_hash(), uniform_hash(), size).prop_map(|entries| {
        entries
            .into_iter()
            .map(|(key, value)| TreeEntry { key, value })
            .collect()
    })
}

fn gen_reads() -> impl Strategy<Value = Vec<H256>> {
    proptest::collection::vec(uniform_hash(), 0..=MAX_ENTRIES)
}

fn gen_updates() -> impl Strategy<Value = Vec<(Index, H256)>> {
    proptest::collection::vec((any::<Index>(), uniform_hash()), 0..=MAX_ENTRIES)
}

fn merge_updates(
    inserts: &mut Vec<TreeEntry>,
    prev_entries: &[TreeEntry],
    updates: Vec<(Index, H256)>,
) {
    // We need deduplication to uphold the tree extension contract.
    let deduplicated_updates: HashMap<_, _> = updates
        .into_iter()
        .map(|(idx, value)| (idx.get(prev_entries).key, value))
        .collect();

    inserts.extend(
        deduplicated_updates
            .into_iter()
            .map(|(key, value)| TreeEntry { key, value }),
    );
}

fn merge_reads(reads: &mut Vec<H256>, prev_entries: &[TreeEntry], indices: Vec<Index>) {
    let deduplicated_reads: HashSet<_> = indices
        .into_iter()
        .map(|idx| idx.get(prev_entries).key)
        .collect();
    reads.extend(deduplicated_reads);
}

fn latest_tree_info(tree: &MerkleTree<PatchSet>) -> Option<BatchOutput> {
    if let Some(version) = tree.latest_version().unwrap() {
        let (root_hash, leaf_count) = tree.root_info(version).unwrap().expect("no latest info");
        Some(BatchOutput {
            root_hash,
            leaf_count,
        })
    } else {
        None
    }
}

fn flip_bit(hash: &mut H256, bit: u8) {
    let (byte, shift_in_byte) = (bit / 8, bit % 8);
    hash.as_bytes_mut()[usize::from(byte)] ^= 1 << shift_in_byte;
}

fn test_read_proof(
    tree: &mut MerkleTree<PatchSet>,
    prev_writes: &[TreeEntry],
    reads: &[H256],
) -> Result<(), TestCaseError> {
    // Necessary for proof fields to be non-empty
    assert!(!prev_writes.is_empty());
    assert!(!reads.is_empty());

    let output = tree.extend(prev_writes).unwrap();
    let version = tree.latest_version().unwrap().expect("no versions");
    let proof = tree.prove(version, reads).unwrap();

    let verify_result = proof.verify_reads(
        &Blake2Hasher,
        <DefaultTreeParams>::TREE_DEPTH,
        output,
        reads,
    );
    let tree_view = verify_result.map_err(|err| TestCaseError::fail(format!("{err:#}")))?;
    prop_assert_eq!(tree_view.root_hash, output.root_hash);
    Ok(())
}

#[derive(Debug)]
enum LeafMutation {
    FlipKeyBit(u8),
    FlipValueBit(u8),
    AddToNext(NonZeroI8),
}

impl LeafMutation {
    fn gen() -> impl Strategy<Value = Self> {
        prop_oneof![
            any::<u8>().prop_map(Self::FlipKeyBit),
            any::<u8>().prop_map(Self::FlipValueBit),
            any::<NonZeroI8>().prop_map(Self::AddToNext),
        ]
    }

    fn apply(self, leaf: &mut Leaf) {
        match self {
            Self::FlipKeyBit(bit) => flip_bit(&mut leaf.key, bit),
            Self::FlipValueBit(bit) => flip_bit(&mut leaf.value, bit),
            Self::AddToNext(value) => {
                leaf.next_index = leaf.next_index.wrapping_add_signed(value.get().into());
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OpMutation {
    flip_hit_or_miss: bool,
    index_increment: i8,
}

impl OpMutation {
    fn gen() -> impl Strategy<Value = Self> {
        (proptest::bool::ANY, proptest::num::i8::ANY).prop_filter_map(
            "no-op",
            |(flip_hit_or_miss, index_increment)| {
                (flip_hit_or_miss || index_increment != 0).then_some(Self {
                    flip_hit_or_miss,
                    index_increment,
                })
            },
        )
    }

    fn apply(self, op: &mut TreeOperation) {
        if self.flip_hit_or_miss {
            *op = match *op {
                TreeOperation::Hit { index } => TreeOperation::Miss { prev_index: index },
                TreeOperation::Miss { prev_index } => TreeOperation::Hit { index: prev_index },
            };
        }
        let index = match op {
            TreeOperation::Hit { index } | TreeOperation::Miss { prev_index: index } => index,
        };
        *index = index.wrapping_add_signed(self.index_increment.into());
    }
}

#[derive(Debug)]
enum ProofMutation {
    MutateOp(Index, OpMutation),
    MutateLeaf(Index, LeafMutation),
    RemoveLeaf(Index),
    FlipHashBit(Index, u8),
    RemoveHash(Index),
}

impl ProofMutation {
    fn gen() -> impl Strategy<Value = Self> {
        prop_oneof![
            (any::<Index>(), OpMutation::gen())
                .prop_map(|(idx, mutation)| Self::MutateOp(idx, mutation)),
            (any::<Index>(), LeafMutation::gen())
                .prop_map(|(idx, mutation)| Self::MutateLeaf(idx, mutation)),
            any::<Index>().prop_map(Self::RemoveLeaf),
            (any::<Index>(), any::<u8>()).prop_map(|(idx, bit)| Self::FlipHashBit(idx, bit)),
            any::<Index>().prop_map(Self::RemoveHash),
        ]
    }

    fn apply(mut self, proof: &mut BatchTreeProof) {
        if proof.hashes.is_empty() {
            // Prevent panics when converting hash indexes below.
            match self {
                Self::FlipHashBit(idx, bit) => {
                    self = Self::MutateLeaf(idx, LeafMutation::FlipKeyBit(bit));
                }
                Self::RemoveHash(idx) => {
                    self = Self::RemoveLeaf(idx);
                }
                _ => { /* Do nothing */ }
            }
        }

        match self {
            Self::MutateOp(idx, mutation) => {
                let op = idx.get_mut(&mut proof.read_operations);
                mutation.apply(op);
            }
            Self::MutateLeaf(idx, mutation) => {
                let idx = idx.index(proof.sorted_leaves.len());
                let leaf = proof.sorted_leaves.values_mut().nth(idx).unwrap();
                mutation.apply(leaf);
            }
            Self::RemoveLeaf(idx) => {
                let idx = idx.index(proof.sorted_leaves.len());
                let leaf_idx = *proof.sorted_leaves.keys().nth(idx).unwrap();
                proof.sorted_leaves.remove(&leaf_idx).unwrap();
            }

            Self::FlipHashBit(idx, bit) => {
                let hash = idx.get_mut(&mut proof.hashes);
                flip_bit(&mut hash.value, bit);
            }
            Self::RemoveHash(idx) => {
                let idx = idx.index(proof.hashes.len());
                proof.hashes.remove(idx);
            }
        }
    }
}

fn test_proof_mutation(
    tree: &mut MerkleTree<PatchSet>,
    prev_writes: &[TreeEntry],
    reads: &[H256],
    mutation: ProofMutation,
) -> Result<(), TestCaseError> {
    // Necessary for proof fields to be non-empty
    assert!(!prev_writes.is_empty());
    assert!(!reads.is_empty());

    let output = tree.extend(prev_writes).unwrap();
    let version = tree.latest_version().unwrap().expect("no versions");
    let mut proof = tree.prove(version, reads).unwrap();

    mutation.apply(&mut proof);
    let verify_result = proof.verify_reads(
        &Blake2Hasher,
        <DefaultTreeParams>::TREE_DEPTH,
        output,
        reads,
    );
    prop_assert!(verify_result.is_err());
    Ok(())
}

fn test_update(
    tree: &mut MerkleTree<PatchSet>,
    writes: &[TreeEntry],
    reads: &[H256],
) -> Result<(), TestCaseError> {
    let tree_info = latest_tree_info(tree);
    let (output, proof) = tree.extend_with_proof(writes, reads).unwrap();

    if tree_info.is_none() {
        let expected_hash = naive_hash_tree(writes);
        prop_assert_eq!(output.root_hash, expected_hash);
    }

    let verify_result = proof.verify(
        &Blake2Hasher,
        <DefaultTreeParams>::TREE_DEPTH,
        tree_info,
        writes,
        reads,
    );
    let tree_view = verify_result.map_err(|err| TestCaseError::fail(format!("{err:#}")))?;
    prop_assert_eq!(tree_view.root_hash, output.root_hash);
    Ok(())
}

proptest! {
    #[test]
    fn verifying_update_proof_for_empty_tree(
        writes in gen_writes(0..=MAX_ENTRIES),
        reads in gen_reads(),
    ) {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        test_update(&mut tree, &writes, &reads)?;
    }

    #[test]
    fn verifying_update_proof_for_filled_tree(
        prev_entries in gen_writes(1..=MAX_ENTRIES),
        inserts in gen_writes(0..=MAX_ENTRIES),
        missing_reads in gen_reads(),
    ) {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        tree.extend(&prev_entries).unwrap();
        test_update(&mut tree, &inserts, &missing_reads)?;
    }

    #[test]
    fn verifying_update_proof_for_filled_tree_with_updates(
        prev_entries in gen_writes(1..=MAX_ENTRIES),
        inserts in gen_writes(0..=MAX_ENTRIES),
        missing_reads in gen_reads(),
        updates in gen_updates(),
        read_indices in proptest::collection::vec(any::<Index>(), 0..=MAX_ENTRIES),
    ) {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        tree.extend(&prev_entries).unwrap();

        let mut all_writes = inserts;
        merge_updates(&mut all_writes, &prev_entries, updates);
        let mut all_reads = missing_reads;
        merge_reads(&mut all_reads, &prev_entries, read_indices);
        test_update(&mut tree, &all_writes, &all_reads)?;
    }

    #[test]
    fn verifying_read_proof(
        prev_entries in gen_writes(1..=MAX_ENTRIES),
        read_indices in proptest::collection::vec(any::<Index>(), 1..=MAX_ENTRIES),
        missing_reads in gen_reads(),
    ) {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        let mut all_reads = missing_reads;
        merge_reads(&mut all_reads, &prev_entries, read_indices);
        test_read_proof(&mut tree, &prev_entries, &all_reads)?;
    }

    #[test]
    fn mutating_read_proof(
        prev_entries in gen_writes(1..=MAX_ENTRIES),
        read_indices in proptest::collection::vec(any::<Index>(), 1..=MAX_ENTRIES),
        missing_reads in gen_reads(),
        mutation in ProofMutation::gen(),
    ) {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        let mut all_reads = missing_reads;
        merge_reads(&mut all_reads, &prev_entries, read_indices);
        test_proof_mutation(&mut tree, &prev_entries, &all_reads, mutation)?;
    }
}
