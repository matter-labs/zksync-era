//! Tests not tied to the zksync domain.

use once_cell::sync::Lazy;
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use zksync_crypto::hasher::{blake2::Blake2Hasher, Hasher};
use zksync_merkle_tree::{
    Database, HashTree, MerkleTree, PatchSet, Patched, TreeInstruction, TreeLogEntry,
};
use zksync_types::{AccountTreeId, Address, StorageKey, H256, U256};

use crate::common::generate_key_value_pairs;

fn convert_to_writes(kvs: &[(U256, H256)]) -> Vec<(U256, TreeInstruction)> {
    let kvs = kvs
        .iter()
        .map(|&(key, hash)| (key, TreeInstruction::Write(hash)));
    kvs.collect()
}

// The extended version of computations used in `InternalNode`.
fn compute_tree_hash(kvs: &[(U256, H256)]) -> H256 {
    assert!(!kvs.is_empty());

    let hasher = Blake2Hasher;
    let mut empty_tree_hash = hasher.hash_bytes(&[0_u8; 40]);
    let level = kvs.iter().enumerate().map(|(i, (key, value))| {
        let leaf_index = i as u64 + 1;
        let mut bytes = [0_u8; 40];
        bytes[..8].copy_from_slice(&leaf_index.to_be_bytes());
        bytes[8..].copy_from_slice(value.as_ref());
        (*key, hasher.hash_bytes(&bytes))
    });
    let mut level: Vec<(U256, H256)> = level.collect();
    level.sort_unstable_by_key(|(key, _)| *key);

    for _ in 0..256 {
        let mut next_level = vec![];
        let mut i = 0;
        while i < level.len() {
            let (pos, hash) = level[i];
            let aggregate_hash = if pos.bit(0) {
                // `pos` corresponds to a right branch of its parent
                hasher.compress(&empty_tree_hash, &hash)
            } else if let Some((next_pos, next_hash)) = level.get(i + 1) {
                if pos + 1 == *next_pos {
                    i += 1;
                    hasher.compress(&hash, next_hash)
                } else {
                    hasher.compress(&hash, &empty_tree_hash)
                }
            } else {
                hasher.compress(&hash, &empty_tree_hash)
            };
            next_level.push((pos >> 1, aggregate_hash));
            i += 1;
        }

        level = next_level;
        empty_tree_hash = hasher.compress(&empty_tree_hash, &empty_tree_hash);
    }
    level[0].1
}

#[test]
fn compute_tree_hash_works_correctly() {
    // Reference value taken from the previous implementation.
    const EXPECTED_HASH: H256 = H256([
        127, 0, 166, 178, 238, 222, 150, 8, 87, 112, 60, 140, 185, 233, 111, 40, 185, 16, 230, 105,
        52, 18, 206, 164, 176, 6, 242, 66, 57, 182, 129, 224,
    ]);

    let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
    let key = StorageKey::new(AccountTreeId::new(address), H256::zero());
    let key = key.hashed_key_u256();
    let hash = compute_tree_hash(&[(key, H256([1; 32]))]);
    assert_eq!(hash, EXPECTED_HASH);
}

#[test]
fn root_hash_is_computed_correctly_on_empty_tree() {
    for kv_count in [1, 2, 3, 5, 8, 13, 21, 100] {
        println!("Inserting {kv_count} key-value pairs");

        let mut tree = MerkleTree::new(PatchSet::default());
        let kvs = generate_key_value_pairs(0..kv_count);
        let expected_hash = compute_tree_hash(&kvs);
        let output = tree.extend(kvs);
        assert_eq!(output.root_hash, expected_hash);
    }
}

#[test]
fn proofs_are_computed_correctly_on_empty_tree() {
    const RNG_SEED: u64 = 123;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let empty_tree_hash = Blake2Hasher.empty_subtree_hash(256);
    for kv_count in [1, 2, 3, 5, 8, 13, 21, 100] {
        println!("Inserting {kv_count} key-value pairs");

        let mut tree = MerkleTree::new(PatchSet::default());
        let kvs = generate_key_value_pairs(0..kv_count);
        let expected_hash = compute_tree_hash(&kvs);
        let instructions = convert_to_writes(&kvs);
        let output = tree.extend_with_proofs(instructions.clone());

        assert_eq!(output.root_hash(), Some(expected_hash));
        assert_eq!(output.logs.len(), instructions.len());
        output.verify_proofs(&Blake2Hasher, empty_tree_hash, &instructions);
        let root_hash = output.root_hash().unwrap();

        let reads = instructions
            .iter()
            .map(|(key, _)| (*key, TreeInstruction::Read));
        let mut reads: Vec<_> = reads.collect();
        reads.shuffle(&mut rng);
        let output = tree.extend_with_proofs(reads.clone());
        output.verify_proofs(&Blake2Hasher, root_hash, &reads);
        assert_eq!(output.root_hash(), Some(root_hash));
    }
}

#[test]
fn proofs_are_computed_correctly_for_mixed_instructions() {
    const RNG_SEED: u64 = 123;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let mut tree = MerkleTree::new(PatchSet::default());
    let kvs = generate_key_value_pairs(0..20);
    let output = tree.extend(kvs.clone());
    let old_root_hash = output.root_hash;

    let reads = kvs.iter().map(|(key, _)| (*key, TreeInstruction::Read));
    let mut instructions: Vec<_> = reads.collect();
    // Overwrite all keys in the tree.
    let writes: Vec<_> = kvs.iter().map(|(key, _)| (*key, H256::zero())).collect();
    let expected_hash = compute_tree_hash(&writes);
    instructions.extend(convert_to_writes(&writes));
    instructions.shuffle(&mut rng);

    let output = tree.extend_with_proofs(instructions.clone());
    // Check that there are some read ops recorded.
    assert!(output
        .logs
        .iter()
        .any(|op| matches!(op.base, TreeLogEntry::Read { .. })));

    output.verify_proofs(&Blake2Hasher, old_root_hash, &instructions);
    assert_eq!(output.root_hash(), Some(expected_hash));
}

#[test]
fn proofs_are_computed_correctly_for_missing_keys() {
    const RNG_SEED: u64 = 123;

    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    let kvs = generate_key_value_pairs(0..20);
    let mut instructions = convert_to_writes(&kvs);
    let missing_reads = generate_key_value_pairs(20..50)
        .into_iter()
        .map(|(key, _)| (key, TreeInstruction::Read));
    instructions.extend(missing_reads);
    instructions.shuffle(&mut rng);

    let mut tree = MerkleTree::new(PatchSet::default());
    let output = tree.extend_with_proofs(instructions.clone());
    let read_misses = output
        .logs
        .iter()
        .filter(|op| matches!(op.base, TreeLogEntry::ReadMissingKey));
    assert_eq!(read_misses.count(), 30);
    let empty_tree_hash = Blake2Hasher.empty_subtree_hash(256);
    output.verify_proofs(&Blake2Hasher, empty_tree_hash, &instructions);
}

// Computing the expected hash takes some time in the debug mode, so we memoize it.
static KVS_AND_HASH: Lazy<(Vec<(U256, H256)>, H256)> = Lazy::new(|| {
    let kvs = generate_key_value_pairs(0..100);
    let expected_hash = compute_tree_hash(&kvs);
    (kvs, expected_hash)
});

fn test_intermediate_commits(db: &mut impl Database, chunk_size: usize) {
    let (kvs, expected_hash) = &*KVS_AND_HASH;
    let mut final_hash = H256::zero();
    let mut tree = MerkleTree::new(db);
    for chunk in kvs.chunks(chunk_size) {
        let output = tree.extend(chunk.to_vec());
        final_hash = output.root_hash;
    }
    assert_eq!(final_hash, *expected_hash);

    let latest_version = tree.latest_version().unwrap();
    for version in 0..=latest_version {
        tree.verify_consistency(version).unwrap();
    }
}

#[test]
fn root_hash_is_computed_correctly_with_intermediate_commits() {
    for chunk_size in [3, 5, 10, 17, 28, 42] {
        println!("Inserting 100 key-value pairs in {chunk_size}-sized chunks");
        test_intermediate_commits(&mut PatchSet::default(), chunk_size);
    }
}

#[test]
fn proofs_are_computed_correctly_with_intermediate_commits() {
    let (kvs, expected_hash) = &*KVS_AND_HASH;
    for chunk_size in [3, 5, 10, 17, 28, 42] {
        println!("Inserting 100 key-value pairs in {chunk_size}-sized chunks");

        let mut tree = MerkleTree::new(PatchSet::default());
        let mut root_hash = Blake2Hasher.empty_subtree_hash(256);
        for chunk in kvs.chunks(chunk_size) {
            let instructions = convert_to_writes(chunk);
            let output = tree.extend_with_proofs(instructions.clone());
            output.verify_proofs(&Blake2Hasher, root_hash, &instructions);
            root_hash = output.root_hash().unwrap();
        }
        assert_eq!(root_hash, *expected_hash);
    }
}

fn test_accumulated_commits<DB: Database>(db: DB, chunk_size: usize) -> DB {
    let (kvs, expected_hash) = &*KVS_AND_HASH;
    let mut db = Patched::new(db);
    let mut final_hash = H256::zero();
    for chunk in kvs.chunks(chunk_size) {
        let mut tree = MerkleTree::new(&mut db);
        let output = tree.extend(chunk.to_vec());
        final_hash = output.root_hash;
    }
    assert_eq!(final_hash, *expected_hash);

    db.flush();
    let mut db = db.into_inner();
    let tree = MerkleTree::new(&mut db);
    let latest_version = tree.latest_version().unwrap();
    for version in 0..=latest_version {
        tree.verify_consistency(version).unwrap();
    }
    db
}

#[test]
fn accumulating_commits() {
    for chunk_size in [3, 5, 10, 17, 28, 42] {
        println!("Inserting 100 key-value pairs in {chunk_size}-sized chunks");
        test_accumulated_commits(PatchSet::default(), chunk_size);
    }
}

fn test_root_hash_computing_with_reverts(db: &mut impl Database) {
    let (kvs, expected_hash) = &*KVS_AND_HASH;
    let (initial_update, final_update) = kvs.split_at(75);
    let key_updates: Vec<_> = kvs.iter().map(|(key, _)| (*key, H256([255; 32]))).collect();
    let key_inserts = generate_key_value_pairs(100..200);

    let mut tree = MerkleTree::new(db);
    let initial_output = tree.extend(initial_update.to_vec());

    // Try rolling back one block at a time.
    let reverted_updates = key_updates.chunks(25).chain(key_inserts.chunks(25));
    for reverted_update in reverted_updates {
        let reverted_output = tree.extend(reverted_update.to_vec());
        assert_ne!(reverted_output, initial_output);

        tree.truncate_recent_versions(1);
        assert_eq!(tree.latest_version(), Some(0));
        assert_eq!(tree.root_hash(0), Some(initial_output.root_hash));

        let final_output = tree.extend(final_update.to_vec());
        assert_eq!(final_output.root_hash, *expected_hash);
        assert_eq!(tree.latest_version(), Some(1));
        assert_eq!(tree.root_hash(0), Some(initial_output.root_hash));
        assert_eq!(tree.root_hash(1), Some(final_output.root_hash));

        tree.truncate_recent_versions(1);
    }
}

#[test]
fn root_hash_is_computed_correctly_with_reverts() {
    test_root_hash_computing_with_reverts(&mut PatchSet::default());
}

fn test_root_hash_computing_with_key_updates(db: impl Database) {
    const RNG_SEED: u64 = 42;
    const P_SCALE: usize = 1_000;
    // ^ Scaling factor for probabilities (to avoid floating-point conversions)

    let mut kvs = generate_key_value_pairs(0..50);
    let mut tree = MerkleTree::new(db);
    let expected_hash = compute_tree_hash(&kvs);
    let output = tree.extend(kvs.clone());
    assert_eq!(output.root_hash, expected_hash);

    // Overwrite some `kvs` entries and add some new ones.
    let changed_kvs = kvs.iter_mut().enumerate().filter_map(|(i, kv)| {
        if i % 3 == 1 {
            kv.1 = H256::from_low_u64_be((i + 100) as u64);
            return Some(*kv);
        }
        None
    });
    let changed_kvs: Vec<_> = changed_kvs.collect();
    let new_kvs = generate_key_value_pairs(50..75);
    kvs.extend_from_slice(&new_kvs);
    let expected_hash = compute_tree_hash(&kvs);

    // We can merge `changed_kvs` and `new_kvs` in any way that preserves `new_kvs` ordering.
    // We'll do multiple ways (which also will effectively test DB rollbacks).

    // All changed KVs, then all new KVs.
    let mut update = Vec::with_capacity(changed_kvs.len() + new_kvs.len());
    update.extend_from_slice(&changed_kvs);
    update.extend_from_slice(&new_kvs);
    let output = tree.extend(update.clone());
    assert_eq!(output.root_hash, expected_hash);

    // All changed KVs (randomly shuffled), then all new KVs.
    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    update[..changed_kvs.len()].shuffle(&mut rng);
    let output = tree.extend(update);
    assert_eq!(output.root_hash, expected_hash);

    // All new KVs, then all changed KVs.
    let mut update = Vec::with_capacity(changed_kvs.len() + new_kvs.len());
    update.extend_from_slice(&new_kvs);
    update.extend_from_slice(&changed_kvs);
    let output = tree.extend(update);
    assert_eq!(output.root_hash, expected_hash);

    // New KVs and changed KVs randomly spliced.
    let mut update = Vec::with_capacity(changed_kvs.len() + new_kvs.len());
    let changed_p = changed_kvs.len() * P_SCALE / (changed_kvs.len() + new_kvs.len());
    let mut changed_kvs = changed_kvs.into_iter();
    let mut new_kvs = new_kvs.into_iter();
    for _ in 0..(changed_kvs.len() + new_kvs.len()) {
        // We can run out of elements in one of the iterators, but we don't really care.
        if rng.gen_range(0..P_SCALE) <= changed_p {
            update.extend(changed_kvs.next());
        } else {
            update.extend(new_kvs.next());
        }
    }
    update.extend(changed_kvs.chain(new_kvs));

    let output = tree.extend(update);
    assert_eq!(output.root_hash, expected_hash);
}

#[test]
fn root_hash_is_computed_correctly_with_key_updates() {
    test_root_hash_computing_with_key_updates(PatchSet::default());
}

#[test]
fn proofs_are_computed_correctly_with_key_updates() {
    const RNG_SEED: u64 = 1_234;

    let (kvs, expected_hash) = &*KVS_AND_HASH;
    let mut rng = StdRng::seed_from_u64(RNG_SEED);

    for updated_keys in [5, 10, 17, 28, 42] {
        println!("Inserting 100 key-value pairs with {updated_keys} updates");

        let old_instructions: Vec<_> = kvs[..updated_keys]
            .iter()
            .map(|(key, _)| (*key, TreeInstruction::Write(H256([255; 32]))))
            .collect();
        // Move the updated keys to the random places in the `kvs` vector.
        let mut writes = convert_to_writes(kvs);
        let mut instructions = writes.split_off(updated_keys);
        for updated_kv in writes {
            let idx = rng.gen_range(0..=instructions.len());
            instructions.insert(idx, updated_kv);
        }

        let mut tree = MerkleTree::new(PatchSet::default());
        let output = tree.extend_with_proofs(old_instructions.clone());
        let empty_tree_hash = Blake2Hasher.empty_subtree_hash(256);
        output.verify_proofs(&Blake2Hasher, empty_tree_hash, &old_instructions);

        let root_hash = output.root_hash().unwrap();
        let output = tree.extend_with_proofs(instructions.clone());
        assert_eq!(output.root_hash(), Some(*expected_hash));
        output.verify_proofs(&Blake2Hasher, root_hash, &instructions);
    }
}

// Taken from the integration tests for the previous tree implementation.
fn test_root_hash_equals_to_previous_implementation(db: &mut impl Database) {
    const PREV_IMPL_HASH: H256 = H256([
        125, 25, 107, 171, 182, 155, 32, 70, 138, 108, 238, 150, 140, 205, 193, 39, 90, 92, 122,
        233, 118, 238, 248, 201, 160, 55, 58, 206, 244, 216, 188, 10,
    ]);

    let addrs = [
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .into_iter()
    .map(|s| s.parse::<Address>().unwrap());

    let keys = addrs.flat_map(|addr| {
        (0..20).map(move |i| {
            StorageKey::new(AccountTreeId::new(addr), H256::from_low_u64_be(i)).hashed_key_u256()
        })
    });
    let values = (0..100).map(H256::from_low_u64_be);
    let kvs: Vec<_> = keys.zip(values).collect();

    let expected_hash = compute_tree_hash(&kvs);
    assert_eq!(expected_hash, PREV_IMPL_HASH);

    let mut tree = MerkleTree::new(db);
    assert!(tree.latest_version().is_none());
    let output = tree.extend(kvs);
    assert_eq!(output.root_hash, PREV_IMPL_HASH);
    assert_eq!(tree.latest_version(), Some(0));
    assert_eq!(tree.root_hash(0), Some(PREV_IMPL_HASH));
}

#[test]
fn root_hash_equals_to_previous_implementation() {
    test_root_hash_equals_to_previous_implementation(&mut PatchSet::default());
}

/// RocksDB-specific tests.
mod rocksdb {
    use serde::{Deserialize, Serialize};
    use serde_with::{hex::Hex, serde_as};
    use tempfile::TempDir;

    use std::collections::BTreeMap;

    use super::*;
    use zksync_merkle_tree::{MerkleTreeColumnFamily, RocksDBWrapper};
    use zksync_storage::RocksDB;

    #[derive(Debug)]
    struct Harness {
        db: RocksDBWrapper,
        dir: TempDir,
    }

    impl Harness {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed creating temporary dir for RocksDB");
            let db = RocksDBWrapper::new(&dir);
            Self { db, dir }
        }
    }

    type KeyValuePair = (Box<[u8]>, Box<[u8]>);

    #[serde_as]
    #[derive(Debug, Serialize, Deserialize)]
    struct DatabaseSnapshot {
        #[serde_as(as = "BTreeMap<Hex, Hex>")]
        tree: Vec<KeyValuePair>,
        #[serde_as(as = "BTreeMap<Hex, Hex>")]
        stale_keys: Vec<KeyValuePair>,
    }

    impl DatabaseSnapshot {
        fn new(raw_db: &RocksDB<MerkleTreeColumnFamily>) -> Self {
            let cf = MerkleTreeColumnFamily::Tree;
            let tree: Vec<_> = raw_db.prefix_iterator_cf(cf, &[]).collect();
            assert!(!tree.is_empty());
            let cf = MerkleTreeColumnFamily::StaleKeys;
            let stale_keys: Vec<_> = raw_db.prefix_iterator_cf(cf, &[]).collect();
            Self { tree, stale_keys }
        }
    }

    #[test]
    fn root_hash_equals_to_previous_implementation() {
        let mut harness = Harness::new();
        test_root_hash_equals_to_previous_implementation(&mut harness.db);

        // Snapshot the DB storage to ensure its backward compatibility. This works because
        // the storage contents is fully deterministic.
        let raw_db = harness.db.into_inner();
        insta::assert_yaml_snapshot!("db-snapshot", DatabaseSnapshot::new(&raw_db));
    }

    #[test]
    fn root_hash_is_computed_correctly_with_key_updates() {
        let harness = Harness::new();
        test_root_hash_computing_with_key_updates(harness.db);
    }

    #[test]
    fn root_hash_is_computed_correctly_with_intermediate_commits() {
        let Harness { mut db, dir: _dir } = Harness::new();
        for chunk_size in [3, 8, 21] {
            test_intermediate_commits(&mut db, chunk_size);

            let raw_db = db.into_inner();
            let snapshot_name = format!("db-snapshot-{chunk_size}-chunked-commits");
            insta::assert_yaml_snapshot!(snapshot_name, DatabaseSnapshot::new(&raw_db));

            // Clear the entire database instead of using `MerkleTree::truncate_versions()`
            // so that it doesn't contain any junk that can influence snapshots.
            let mut batch = raw_db.new_write_batch();
            let cf = MerkleTreeColumnFamily::Tree;
            batch.delete_range_cf(cf, (&[] as &[_])..&u64::MAX.to_be_bytes());
            raw_db.write(batch).unwrap();
            db = RocksDBWrapper::from(raw_db);
        }
    }

    #[test]
    fn root_hash_is_computed_correctly_with_reverts() {
        let Harness { mut db, dir: _dir } = Harness::new();
        test_root_hash_computing_with_reverts(&mut db);

        let mut tree = MerkleTree::new(&mut db);
        assert_eq!(tree.latest_version(), Some(0));
        tree.extend(vec![]);

        // Check that reverted data is not present in the database.
        let raw_db = db.into_inner();
        let cf = MerkleTreeColumnFamily::Tree;
        let latest_kvs: Vec<_> = raw_db
            .prefix_iterator_cf(cf, &[0, 0, 0, 0, 0, 0, 0, 1])
            .collect();
        // Only the root node should be present.
        assert_eq!(latest_kvs.len(), 1, "{latest_kvs:?}");
    }

    #[test]
    fn accumulating_commits() {
        let Harness { mut db, dir: _dir } = Harness::new();
        for chunk_size in [3, 5, 10, 17, 28, 42] {
            println!("Inserting 100 key-value pairs in {chunk_size}-sized chunks");
            db = test_accumulated_commits(db, chunk_size);
            MerkleTree::new(&mut db).truncate_recent_versions(0);
        }
    }

    #[test]
    #[should_panic(expected = "Mismatch between the provided tree hasher `no_op256`")]
    fn tree_tags_mismatch() {
        let Harness { mut db, dir: _dir } = Harness::new();
        let mut tree = MerkleTree::new(&mut db);
        tree.extend(vec![(U256::zero(), H256::zero())]);

        MerkleTree::with_hasher(&mut db, &());
    }

    #[test]
    #[should_panic(expected = "Mismatch between the provided tree hasher `no_op256`")]
    fn tree_tags_mismatch_with_cold_restart() {
        let Harness { db, dir } = Harness::new();
        let mut tree = MerkleTree::new(db);
        tree.extend(vec![(U256::zero(), H256::zero())]);
        drop(tree);

        let db = RocksDBWrapper::new(dir);
        MerkleTree::with_hasher(db, &());
    }
}
