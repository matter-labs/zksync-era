//! Tests for tree recovery.

use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use test_casing::test_casing;
use zksync_crypto::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{
    recovery::MerkleTreeRecovery, Database, MerkleTree, PatchSet, PruneDatabase, ValueHash,
};

use crate::common::{convert_to_writes, generate_key_value_pairs, TreeMap, ENTRIES_AND_HASH};

#[derive(Debug, Clone, Copy)]
enum RecoveryKind {
    Linear,
    Random,
}

impl RecoveryKind {
    const ALL: [Self; 2] = [Self::Linear, Self::Random];
}

#[test]
fn recovery_basics() {
    let (kvs, expected_hash) = &*ENTRIES_AND_HASH;
    let mut recovery_entries: Vec<_> = kvs.clone();
    recovery_entries.sort_unstable_by_key(|entry| entry.key);
    let greatest_key = recovery_entries[99].key;

    let recovered_version = 123;
    let mut recovery = MerkleTreeRecovery::new(PatchSet::default(), recovered_version).unwrap();
    recovery.extend_linear(recovery_entries).unwrap();

    assert_eq!(recovery.last_processed_key(), Some(greatest_key));
    assert_eq!(recovery.root_hash(), *expected_hash);

    let tree = MerkleTree::new(recovery.finalize().unwrap()).unwrap();
    tree.verify_consistency(recovered_version, true).unwrap();
}

fn test_recovery_in_chunks(mut db: impl PruneDatabase, kind: RecoveryKind, chunk_size: usize) {
    let (kvs, expected_hash) = &*ENTRIES_AND_HASH;
    let mut recovery_entries = kvs.clone();
    if matches!(kind, RecoveryKind::Linear) {
        recovery_entries.sort_unstable_by_key(|entry| entry.key);
    }
    let greatest_key = recovery_entries
        .iter()
        .map(|entry| entry.key)
        .max()
        .unwrap();

    let recovered_version = 123;
    let mut recovery = MerkleTreeRecovery::new(&mut db, recovered_version).unwrap();
    for (i, chunk) in recovery_entries.chunks(chunk_size).enumerate() {
        match kind {
            RecoveryKind::Linear => recovery.extend_linear(chunk.to_vec()).unwrap(),
            RecoveryKind::Random => recovery.extend_random(chunk.to_vec()).unwrap(),
        }
        if i % 3 == 1 {
            recovery = MerkleTreeRecovery::new(&mut db, recovered_version).unwrap();
            // ^ Simulate recovery interruption and restart
        }
    }

    assert_eq!(recovery.last_processed_key(), Some(greatest_key));
    assert_eq!(recovery.root_hash(), *expected_hash);

    let mut tree = MerkleTree::new(recovery.finalize().unwrap()).unwrap();
    tree.verify_consistency(recovered_version, true).unwrap();
    // Check that new tree versions can be built and function as expected.
    test_tree_after_recovery(&mut tree, recovered_version, *expected_hash);
}

fn test_tree_after_recovery<DB: Database>(
    tree: &mut MerkleTree<DB>,
    recovered_version: u64,
    root_hash: ValueHash,
) {
    const RNG_SEED: u64 = 765;
    const CHUNK_SIZE: usize = 18;

    assert_eq!(tree.latest_version(), Some(recovered_version));
    assert_eq!(tree.root_hash(recovered_version), Some(root_hash));
    for ver in 0..recovered_version {
        assert_eq!(tree.root_hash(ver), None);
    }

    // Check adding new and updating existing entries in the tree.
    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let mut kvs = generate_key_value_pairs(100..=150);
    let mut modified_kvs = generate_key_value_pairs(50..=100);
    for entry in &mut modified_kvs {
        entry.value = ValueHash::repeat_byte(1);
    }
    modified_kvs.shuffle(&mut rng);
    kvs.extend(modified_kvs);

    let mut tree_map = TreeMap::new(&ENTRIES_AND_HASH.0);
    let mut prev_root_hash = root_hash;
    for (i, chunk) in kvs.chunks(CHUNK_SIZE).enumerate() {
        tree_map.extend(chunk);

        let new_root_hash = if i % 2 == 0 {
            let output = tree.extend(chunk.to_vec()).unwrap();
            output.root_hash
        } else {
            let instructions = convert_to_writes(chunk);
            let output = tree.extend_with_proofs(instructions.clone()).unwrap();
            output
                .verify_proofs(&Blake2Hasher, prev_root_hash, &instructions)
                .unwrap();
            output.root_hash().unwrap()
        };

        assert_eq!(new_root_hash, tree_map.root_hash());
        tree.verify_consistency(recovered_version + i as u64, true)
            .unwrap();
        prev_root_hash = new_root_hash;
    }
}

fn test_parallel_recovery_in_chunks<DB>(db: DB, kind: RecoveryKind, chunk_size: usize)
where
    DB: PruneDatabase + Clone + 'static,
{
    let (kvs, expected_hash) = &*ENTRIES_AND_HASH;
    let mut recovery_entries = kvs.clone();
    if matches!(kind, RecoveryKind::Linear) {
        recovery_entries.sort_unstable_by_key(|entry| entry.key);
    }

    let recovered_version = 123;
    let mut recovery = MerkleTreeRecovery::new(db.clone(), recovered_version).unwrap();
    recovery.parallelize_persistence(4).unwrap();
    for (i, chunk) in recovery_entries.chunks(chunk_size).enumerate() {
        match kind {
            RecoveryKind::Linear => recovery.extend_linear(chunk.to_vec()).unwrap(),
            RecoveryKind::Random => recovery.extend_random(chunk.to_vec()).unwrap(),
        }
        if i % 3 == 1 {
            // need this to ensure that the old persistence thread doesn't corrupt DB
            recovery.wait_for_persistence().unwrap();
            recovery = MerkleTreeRecovery::new(db.clone(), recovered_version).unwrap();
            recovery.parallelize_persistence(4).unwrap();
            // ^ Simulate recovery interruption and restart.
        }
    }

    let mut tree = MerkleTree::new(recovery.finalize().unwrap()).unwrap();
    tree.verify_consistency(recovered_version, true).unwrap();
    // Check that new tree versions can be built and function as expected.
    test_tree_after_recovery(&mut tree, recovered_version, *expected_hash);
}

#[test_casing(8, test_casing::Product((RecoveryKind::ALL, [6, 10, 17, 42])))]
fn recovery_in_chunks(kind: RecoveryKind, chunk_size: usize) {
    test_recovery_in_chunks(PatchSet::default(), kind, chunk_size);
}

mod rocksdb {
    use tempfile::TempDir;
    use zksync_merkle_tree::RocksDBWrapper;

    use super::*;

    #[test_casing(8, test_casing::Product((RecoveryKind::ALL, [6, 10, 17, 42])))]
    fn recovery_in_chunks(kind: RecoveryKind, chunk_size: usize) {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        test_recovery_in_chunks(db, kind, chunk_size);
    }

    #[test_casing(8, test_casing::Product((RecoveryKind::ALL, [6, 10, 17, 42])))]
    fn parallel_recovery_in_chunks(kind: RecoveryKind, chunk_size: usize) {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();
        test_parallel_recovery_in_chunks(db, kind, chunk_size);
    }
}
