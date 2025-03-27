//! Property tests for the Merkle tree.

use std::{
    collections::{HashMap, HashSet},
    ops,
};

use proptest::{prelude::*, sample::Index};
use zksync_basic_types::H256;
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;

use super::naive_hash_tree;
use crate::{BatchOutput, DefaultTreeParams, MerkleTree, PatchSet, TreeEntry, TreeParams};

const MAX_ENTRIES: usize = 100;

fn uniform_hash() -> impl Strategy<Value = H256> {
    proptest::array::uniform32(proptest::num::u8::ANY)
        .prop_map(H256)
        .prop_filter("guard", |hash| {
            *hash != H256::zero() && *hash != H256::repeat_byte(0xff)
        })
}

fn gen_writes(size: ops::RangeInclusive<usize>) -> impl Strategy<Value = Vec<TreeEntry>> {
    let entry = (uniform_hash(), uniform_hash()).prop_map(|(key, value)| TreeEntry { key, value });
    proptest::collection::vec(entry, size)
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

fn test_update(
    tree: &mut MerkleTree<PatchSet>,
    writes: &[TreeEntry],
    reads: &[H256],
) -> Result<(), TestCaseError> {
    let tree_info = if let Some(version) = tree.latest_version().unwrap() {
        let (root_hash, leaf_count) = tree.root_info(version).unwrap().expect("no latest info");
        Some(BatchOutput {
            root_hash,
            leaf_count,
        })
    } else {
        None
    };

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
}
