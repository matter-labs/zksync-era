//! Domain-specific tests. Taken almost verbatim from the previous tree implementation.

use std::slice;

use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use tempfile::TempDir;
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::{domain::ZkSyncTree, HashTree, TreeEntry, TreeInstruction};
use zksync_prover_interface::inputs::StorageLogMetadata;
use zksync_storage::RocksDB;
use zksync_system_constants::ACCOUNT_CODE_STORAGE_ADDRESS;
use zksync_types::{AccountTreeId, Address, L1BatchNumber, StorageKey, H256};

fn gen_storage_logs() -> Vec<TreeInstruction> {
    let addrs = vec![
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .into_iter()
    .map(|s| s.parse::<Address>().unwrap());

    let proof_keys = addrs.flat_map(|addr| {
        (0..20).map(move |i| StorageKey::new(AccountTreeId::new(addr), H256::from_low_u64_be(i)))
    });
    let proof_values = (0..100).map(H256::from_low_u64_be);

    proof_keys
        .zip(proof_values)
        .enumerate()
        .map(|(i, (proof_key, proof_value))| {
            let entry = TreeEntry::new(proof_key.hashed_key_u256(), i as u64 + 1, proof_value);
            TreeInstruction::Write(entry)
        })
        .collect()
}

#[test]
fn basic_workflow() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();

    let (metadata, expected_root_hash) = {
        let db = RocksDB::new(temp_dir.as_ref()).unwrap();
        let mut tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
        let metadata = tree.process_l1_batch(&logs).unwrap();
        tree.save().unwrap();
        tree.verify_consistency(L1BatchNumber(0)).unwrap();
        (metadata, tree.root_hash())
    };

    assert_eq!(metadata.root_hash, expected_root_hash);
    assert_eq!(metadata.rollup_last_leaf_index, 101);

    assert_eq!(
        expected_root_hash,
        H256([
            125, 25, 107, 171, 182, 155, 32, 70, 138, 108, 238, 150, 140, 205, 193, 39, 90, 92,
            122, 233, 118, 238, 248, 201, 160, 55, 58, 206, 244, 216, 188, 10
        ]),
    );

    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
    tree.verify_consistency(L1BatchNumber(0)).unwrap();
    assert_eq!(tree.root_hash(), expected_root_hash);
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(1));
}

#[test]
fn basic_workflow_multiblock() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();
    let blocks = logs.chunks(9);

    let expected_root_hash = {
        let db = RocksDB::new(temp_dir.as_ref()).unwrap();
        let mut tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
        tree.use_dedicated_thread_pool(2);
        for block in blocks {
            tree.process_l1_batch(block).unwrap();
        }
        tree.save().unwrap();
        tree.root_hash()
    };

    assert_eq!(
        expected_root_hash,
        H256([
            125, 25, 107, 171, 182, 155, 32, 70, 138, 108, 238, 150, 140, 205, 193, 39, 90, 92,
            122, 233, 118, 238, 248, 201, 160, 55, 58, 206, 244, 216, 188, 10
        ]),
    );

    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
    assert_eq!(tree.root_hash(), expected_root_hash);
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(12));
}

#[test]
fn tree_with_single_leaf_works_correctly() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let storage_logs = gen_storage_logs();
    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    {
        let mut tree = ZkSyncTree::new(db.clone().into()).unwrap();
        tree.process_l1_batch(&storage_logs[0..1]).unwrap();
        tree.save().unwrap();
    }
    let mut tree = ZkSyncTree::new(db.into()).unwrap();
    tree.verify_consistency(L1BatchNumber(0)).unwrap();

    // Add more logs to the tree.
    for single_log_slice in storage_logs[1..].chunks(1) {
        tree.process_l1_batch(single_log_slice).unwrap();
        tree.save().unwrap();
    }
    assert_eq!(
        tree.root_hash(),
        H256([
            125, 25, 107, 171, 182, 155, 32, 70, 138, 108, 238, 150, 140, 205, 193, 39, 90, 92,
            122, 233, 118, 238, 248, 201, 160, 55, 58, 206, 244, 216, 188, 10
        ]),
    );
}

#[test]
fn filtering_out_no_op_writes() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let mut tree = ZkSyncTree::new(db.into()).unwrap();
    let mut logs = gen_storage_logs();
    let root_hash = tree.process_l1_batch(&logs).unwrap().root_hash;
    tree.save().unwrap();

    // All writes are no-op updates and thus must be filtered out.
    let new_metadata = tree.process_l1_batch(&logs).unwrap();
    assert_eq!(new_metadata.root_hash, root_hash);
    let merkle_paths = new_metadata.witness.unwrap().into_merkle_paths();
    assert_eq!(merkle_paths.len(), 0);

    // Add some actual repeated writes.
    let mut expected_writes_count = 0;
    for log in logs.iter_mut().step_by(3) {
        let TreeInstruction::Write(entry) = log else {
            unreachable!("Unexpected instruction: {log:?}");
        };
        entry.value = H256::repeat_byte(0xff);
        expected_writes_count += 1;
    }
    let new_metadata = tree.process_l1_batch(&logs).unwrap();
    assert_ne!(new_metadata.root_hash, root_hash);
    let merkle_paths = new_metadata.witness.unwrap().into_merkle_paths();
    assert_eq!(merkle_paths.len(), expected_writes_count);
    for merkle_path in merkle_paths {
        assert!(!merkle_path.first_write);
        assert_eq!(merkle_path.value_written, [0xff; 32]);
    }
}

#[test]
fn revert_blocks() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();

    // Generate logs and save them to DB.
    // Produce 4 blocks with distinct values and 1 block with modified values from first block
    let block_size: usize = 25;
    let address: Address = "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2".parse().unwrap();
    let proof_keys = (0..100).map(move |i| {
        StorageKey::new(AccountTreeId::new(address), H256::from_low_u64_be(i)).hashed_key_u256()
    });
    let proof_values = (0..100).map(H256::from_low_u64_be);

    // Add a couple of blocks of distinct keys/values
    let mut logs: Vec<_> = proof_keys
        .zip(proof_values)
        .map(|(proof_key, proof_value)| {
            let entry = TreeEntry::new(proof_key, proof_value.to_low_u64_be() + 1, proof_value);
            TreeInstruction::Write(entry)
        })
        .collect();
    // Add a block with repeated keys
    let extra_logs = (0..block_size).map(move |i| {
        let key = StorageKey::new(AccountTreeId::new(address), H256::from_low_u64_be(i as u64))
            .hashed_key_u256();
        let entry = TreeEntry::new(key, i as u64 + 1, H256::from_low_u64_be(i as u64 + 1));
        TreeInstruction::Write(entry)
    });
    logs.extend(extra_logs);

    let mirror_logs = logs.clone();
    let tree_metadata: Vec<_> = {
        let mut tree = ZkSyncTree::new(storage.into()).unwrap();
        let metadata = logs.chunks(block_size).map(|chunk| {
            let metadata = tree.process_l1_batch(chunk).unwrap();
            tree.save().unwrap();
            metadata
        });
        metadata.collect()
    };

    assert_eq!(tree_metadata.len(), 5);
    // 4 first blocks must contain only insert ops, while the last one must contain
    // only the update ops.
    for (i, metadata) in tree_metadata.iter().enumerate() {
        let merkle_paths = metadata.witness.clone().unwrap().into_merkle_paths();
        let expected_leaf_index = if i == 4 {
            assert_eq!(merkle_paths.len(), block_size);
            for (merkle_path, idx) in merkle_paths.into_iter().zip(1_u64..) {
                assert!(!merkle_path.first_write);
                assert_eq!(merkle_path.leaf_enumeration_index, idx);
                assert_eq!(merkle_path.value_written, H256::from_low_u64_be(idx).0);
            }
            block_size * 4 + 1
        } else {
            assert_eq!(merkle_paths.len(), block_size);
            for merkle_path in merkle_paths {
                assert!(merkle_path.first_write);
            }
            block_size * (i + 1) + 1
        };
        assert_eq!(metadata.rollup_last_leaf_index, expected_leaf_index as u64);
    }

    // Revert the last block.
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();
    {
        let mut tree = ZkSyncTree::new_lightweight(storage.into()).unwrap();
        assert_eq!(tree.root_hash(), tree_metadata.last().unwrap().root_hash);
        tree.roll_back_logs(L1BatchNumber(3)).unwrap();
        assert_eq!(tree.root_hash(), tree_metadata[3].root_hash);
        tree.save().unwrap();
    }

    // Revert two more blocks.
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();
    {
        let mut tree = ZkSyncTree::new_lightweight(storage.into()).unwrap();
        tree.roll_back_logs(L1BatchNumber(1)).unwrap();
        assert_eq!(tree.root_hash(), tree_metadata[1].root_hash);
        tree.save().unwrap();
    }

    // Revert two more blocks second time; the result should be the same
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();
    {
        let mut tree = ZkSyncTree::new_lightweight(storage.into()).unwrap();
        tree.roll_back_logs(L1BatchNumber(1)).unwrap();
        assert_eq!(tree.root_hash(), tree_metadata[1].root_hash);
        tree.save().unwrap();
    }

    // Reapply one of the reverted logs
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();
    {
        let storage_log = mirror_logs.get(3 * block_size).unwrap();
        let mut tree = ZkSyncTree::new_lightweight(storage.into()).unwrap();
        tree.process_l1_batch(slice::from_ref(storage_log)).unwrap();
        tree.save().unwrap();
    }

    // check saved block number
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();
    let tree = ZkSyncTree::new_lightweight(storage.into()).unwrap();
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(3));
}

#[test]
fn reset_tree() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let storage = RocksDB::new(temp_dir.as_ref()).unwrap();
    let logs = gen_storage_logs();
    let mut tree = ZkSyncTree::new_lightweight(storage.into()).unwrap();
    let empty_root_hash = tree.root_hash();

    logs.chunks(5).fold(empty_root_hash, |hash, chunk| {
        tree.process_l1_batch(chunk).unwrap();
        tree.reset();
        assert_eq!(tree.root_hash(), hash);

        tree.process_l1_batch(chunk).unwrap();
        tree.save().unwrap();
        tree.root_hash()
    });
}

#[test]
fn read_logs() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let mut logs = gen_storage_logs();
    logs.truncate(5);

    let write_metadata = {
        let db = RocksDB::new(temp_dir.as_ref()).unwrap();
        let mut tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
        let metadata = tree.process_l1_batch(&logs).unwrap();
        tree.save().unwrap();
        metadata
    };

    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let mut tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
    let read_logs: Vec<_> = logs
        .into_iter()
        .map(|instr| TreeInstruction::Read(instr.key()))
        .collect();
    let read_metadata = tree.process_l1_batch(&read_logs).unwrap();

    assert_eq!(read_metadata.root_hash, write_metadata.root_hash);
}

fn create_write_log(
    leaf_index: u64,
    address: Address,
    address_storage_key: [u8; 32],
    value: [u8; 32],
) -> TreeInstruction {
    let key = StorageKey::new(AccountTreeId::new(address), H256(address_storage_key));
    TreeInstruction::Write(TreeEntry::new(
        key.hashed_key_u256(),
        leaf_index,
        H256(value),
    ))
}

fn subtract_from_max_value(diff: u8) -> [u8; 32] {
    let mut value = [255_u8; 32];
    value[31] -= diff;
    value
}

#[test]
fn root_hash_compatibility() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let mut tree = ZkSyncTree::new_lightweight(db.into()).unwrap();
    assert_eq!(
        tree.root_hash(),
        H256([
            152, 164, 142, 78, 209, 115, 97, 136, 56, 74, 232, 167, 157, 210, 28, 77, 102, 135,
            229, 253, 34, 202, 24, 20, 137, 6, 215, 135, 54, 192, 216, 106
        ]),
    );

    let storage_logs = vec![
        create_write_log(1, ACCOUNT_CODE_STORAGE_ADDRESS, [0; 32], [1; 32]),
        create_write_log(
            2,
            Address::from_low_u64_be(9223372036854775808),
            [254; 32],
            subtract_from_max_value(1),
        ),
        create_write_log(
            3,
            Address::from_low_u64_be(9223372036854775809),
            [253; 32],
            subtract_from_max_value(2),
        ),
        create_write_log(
            4,
            Address::from_low_u64_be(9223372036854775810),
            [252; 32],
            subtract_from_max_value(3),
        ),
        create_write_log(
            5,
            Address::from_low_u64_be(9223372036854775811),
            [251; 32],
            subtract_from_max_value(4),
        ),
        create_write_log(
            6,
            Address::from_low_u64_be(9223372036854775812),
            [250; 32],
            subtract_from_max_value(5),
        ),
    ];

    let metadata = tree.process_l1_batch(&storage_logs).unwrap();
    assert_eq!(
        metadata.root_hash,
        H256([
            35, 191, 235, 50, 17, 223, 143, 160, 240, 38, 139, 111, 221, 156, 42, 29, 72, 90, 196,
            198, 72, 13, 219, 88, 59, 250, 94, 112, 221, 3, 44, 171
        ])
    );
}

#[test]
fn process_block_idempotency_check() {
    let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");
    let rocks_db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let mut tree = ZkSyncTree::new_lightweight(rocks_db.into()).unwrap();
    let logs = gen_storage_logs();
    let tree_metadata = tree.process_l1_batch(&logs).unwrap();

    // Simulate server restart by calling `process_block` again on the same tree
    tree.reset();
    let repeated_tree_metadata = tree.process_l1_batch(&logs).unwrap();
    assert_eq!(repeated_tree_metadata.root_hash, tree_metadata.root_hash);
    assert_eq!(
        repeated_tree_metadata.rollup_last_leaf_index,
        tree_metadata.rollup_last_leaf_index
    );
}

/// Serializable version of [`StorageLogMetadata`] that hex-encodes byte arrays.
#[serde_as]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct StorageLogMetadataSnapshot {
    #[serde_as(as = "Hex")]
    root_hash: [u8; 32],
    is_write: bool,
    first_write: bool,
    #[serde_as(as = "Vec<Hex>")]
    merkle_paths: Vec<[u8; 32]>,
    #[serde_as(as = "Hex")]
    leaf_hashed_key: [u8; 32],
    leaf_enumeration_index: u64,
    #[serde_as(as = "Hex")]
    value_written: [u8; 32],
    #[serde_as(as = "Hex")]
    value_read: [u8; 32],
}

impl From<StorageLogMetadata> for StorageLogMetadataSnapshot {
    fn from(metadata: StorageLogMetadata) -> Self {
        let mut leaf_hashed_key = [0_u8; 32];
        metadata.leaf_hashed_key.to_big_endian(&mut leaf_hashed_key);

        Self {
            root_hash: metadata.root_hash,
            is_write: metadata.is_write,
            first_write: metadata.first_write,
            merkle_paths: metadata.merkle_paths,
            leaf_hashed_key,
            leaf_enumeration_index: metadata.leaf_enumeration_index,
            value_written: metadata.value_written,
            value_read: metadata.value_read,
        }
    }
}

// Snapshots are taken from the old tree implementation.
#[test]
fn witness_workflow() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();
    let (first_chunk, _) = logs.split_at(logs.len() / 2);

    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let mut tree = ZkSyncTree::new(db.into()).unwrap();
    let metadata = tree.process_l1_batch(first_chunk).unwrap();
    let job = metadata.witness.unwrap();
    assert_eq!(job.next_enumeration_index(), 1);
    let merkle_paths: Vec<_> = job.into_merkle_paths().collect();

    assert!(merkle_paths.iter().all(|log| log.merkle_paths.len() == 256));

    let mut witnesses: Vec<_> = merkle_paths
        .into_iter()
        .map(StorageLogMetadataSnapshot::from)
        .collect();

    // The working dir for integration tests is set to the crate dir, so specifying relative paths
    // should be OK.
    insta::assert_yaml_snapshot!("log-metadata-full", witnesses.last().unwrap());

    for witness in &mut witnesses {
        // Leave only 8 hashes closest to the tree root
        witness.merkle_paths = witness.merkle_paths.split_off(248);
    }
    insta::assert_yaml_snapshot!("log-metadata-list-short", witnesses);
}

#[test]
fn witnesses_with_multiple_blocks() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();

    let db = RocksDB::new(temp_dir.as_ref()).unwrap();
    let mut tree = ZkSyncTree::new(db.into()).unwrap();
    let empty_tree_hashes: Vec<_> = (0..256)
        .map(|i| Blake2Hasher.empty_subtree_hash(i))
        .collect();

    let non_empty_levels_by_block = logs.chunks(10).map(|block| {
        let metadata = tree.process_l1_batch(block).unwrap();
        let witness = metadata.witness.unwrap();

        let non_empty_levels = witness.into_merkle_paths().map(|log| {
            assert_eq!(log.merkle_paths.len(), 256);
            log.merkle_paths
                .iter()
                .zip(&empty_tree_hashes)
                .position(|(hash, empty_hash)| *hash != empty_hash.0)
                .unwrap_or(256)
        });
        non_empty_levels.collect::<Vec<_>>()
    });
    let non_empty_levels_by_block: Vec<_> = non_empty_levels_by_block.collect();

    // For at least some blocks, the non-empty level for the first log can be lower
    // than for the following logs (meaning that additional empty subtree hashes
    // are included in the Merkle path for the following log).
    let has_following_log_with_greater_level =
        non_empty_levels_by_block.iter().any(|block_levels| {
            block_levels[1..]
                .iter()
                .any(|&level| level > block_levels[0])
        });
    assert!(
        has_following_log_with_greater_level,
        "{non_empty_levels_by_block:?}"
    );
}
