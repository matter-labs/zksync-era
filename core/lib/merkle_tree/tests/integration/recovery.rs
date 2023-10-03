//! Tests for tree recovery.

use zksync_merkle_tree::{
    recovery::{MerkleTreeRecovery, RecoveryEntry},
    PatchSet,
};

use crate::common::KVS_AND_HASH;

#[test]
fn recovery_basics() {
    let (kvs, expected_hash) = &*KVS_AND_HASH;
    let recovery_entries = kvs
        .iter()
        .enumerate()
        .map(|(i, &(key, value))| RecoveryEntry {
            key,
            value,
            leaf_index: i as u64 + 1,
        });
    let mut recovery_entries: Vec<_> = recovery_entries.collect();
    recovery_entries.sort_unstable_by_key(|entry| entry.key);
    let greatest_key = recovery_entries[99].key;

    let recovered_version = 123;
    let mut recovery = MerkleTreeRecovery::new(PatchSet::default(), recovered_version);
    recovery.extend(recovery_entries);

    assert_eq!(recovery.last_processed_key(), Some(greatest_key));
    assert_eq!(recovery.root_hash(), *expected_hash);

    let tree = recovery.finalize();
    tree.verify_consistency(recovered_version).unwrap();
}

#[test]
fn recovery_in_chunks() {
    let (kvs, expected_hash) = &*KVS_AND_HASH;
    let recovery_entries = kvs
        .iter()
        .enumerate()
        .map(|(i, &(key, value))| RecoveryEntry {
            key,
            value,
            leaf_index: i as u64 + 1,
        });
    let mut recovery_entries: Vec<_> = recovery_entries.collect();
    recovery_entries.sort_unstable_by_key(|entry| entry.key);
    let greatest_key = recovery_entries[99].key;

    let recovered_version = 123;
    for chunk_size in [6, 10, 17, 42] {
        let mut db = PatchSet::default();
        let mut recovery = MerkleTreeRecovery::new(&mut db, recovered_version);
        for (i, chunk) in recovery_entries.chunks(chunk_size).enumerate() {
            recovery.extend(chunk.to_vec());
            if i % 3 == 1 {
                recovery = MerkleTreeRecovery::new(&mut db, recovered_version);
                // ^ Simulate recovery interruption and restart
            }
        }

        assert_eq!(recovery.last_processed_key(), Some(greatest_key));
        assert_eq!(recovery.root_hash(), *expected_hash);

        let tree = recovery.finalize();
        tree.verify_consistency(recovered_version).unwrap();
    }
}
