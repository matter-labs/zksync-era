use crate::tree_config::TreeConfig;
use crate::types::{TreeKey, ZkHash, ZkHasher};
use crate::{TreeMetadata, ZkSyncTree};
use std::str::FromStr;
use tempfile::TempDir;
use zksync_config::constants::ACCOUNT_CODE_STORAGE_ADDRESS;
use zksync_storage::db::Database;
use zksync_storage::RocksDB;
use zksync_types::proofs::StorageLogMetadata;
use zksync_types::{
    AccountTreeId, Address, L1BatchNumber, StorageKey, StorageLog, WitnessStorageLog, H256,
};
use zksync_utils::u32_to_h256;

/// Checks main operations of the tree.
#[test]
fn basic_workflow() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();

    let expected_root_hash = {
        let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
        let mut tree = ZkSyncTree::new(db);
        let _ = tree.process_block(logs);
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

    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let tree = ZkSyncTree::new(db);
    assert_eq!(tree.root_hash(), expected_root_hash);
    assert_eq!(tree.block_number(), 1);
}

/// Checks main operations of the tree on multiple blocks.
#[test]
fn basic_workflow_multiblock() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();
    let blocks: Vec<_> = logs.chunks(9).collect();

    let expected_root_hash = {
        let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
        let mut tree = ZkSyncTree::new(db);
        let _ = tree.process_blocks(blocks);
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

    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let tree = ZkSyncTree::new(db);
    assert_eq!(tree.root_hash(), expected_root_hash);
    assert_eq!(tree.block_number(), 12);
}

/// Checks main operations of the tree.
#[test]
fn multiple_single_block_workflow() {
    let logs = gen_storage_logs();
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    {
        let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
        let mut tree = ZkSyncTree::new(db);

        logs.chunks(5).into_iter().for_each(|chunk| {
            let metadata = tree.process_block(chunk);
            assert_eq!(tree.root_hash(), metadata.root_hash);
            tree.save().unwrap();
        });
    }
    // verify consistency of final result
    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let mirror_tree = ZkSyncTree::new(db);
    mirror_tree.verify_consistency();
}

#[test]
fn revert_blocks() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);

    // generate logs and save them to db
    // produce 4 blocks with distinct values and 1 block with modified values from first block
    let block_size: usize = 25;
    let address = Address::from_str("4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2").unwrap();
    let proof_keys: Vec<_> = (0..100)
        .map(move |i| StorageKey::new(AccountTreeId::new(address), u32_to_h256(i)))
        .collect();
    let proof_values: Vec<_> = (0..100).map(u32_to_h256).collect();

    // add couple of blocks of distinct keys/values
    let mut logs: Vec<_> = convert_logs(
        proof_keys
            .iter()
            .zip(proof_values.iter())
            .map(|(proof_key, &proof_value)| StorageLog::new_write_log(*proof_key, proof_value))
            .collect(),
    );
    // add block with repeated keys
    let mut extra_logs = convert_logs(
        (0..block_size)
            .map(move |i| {
                StorageLog::new_write_log(
                    StorageKey::new(AccountTreeId::new(address), u32_to_h256(i as u32)),
                    u32_to_h256((i + 1) as u32),
                )
            })
            .collect(),
    );
    logs.append(&mut extra_logs);

    let mirror_logs = logs.clone();
    let tree_metadata = {
        let mut tree = ZkSyncTree::new(storage);
        let mut tree_metadata = vec![];
        for chunk in logs.chunks(block_size) {
            tree_metadata.push(tree.process_block(chunk));
            tree.save().unwrap();
        }
        assert_eq!(tree.block_number(), 5);
        tree_metadata
    };

    let witness_tree = TestWitnessTree::new(tree_metadata[3].clone());
    assert!(witness_tree.get_leaf((4 * block_size - 1) as u64).is_some());

    // revert last block
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    {
        let mut tree = ZkSyncTree::new(storage);
        assert_eq!(tree.root_hash(), tree_metadata.last().unwrap().root_hash);
        let logs_to_revert: Vec<_> = mirror_logs
            .iter()
            .take(block_size)
            .map(|log| {
                (
                    log.storage_log.key.hashed_key_u256(),
                    Some(log.storage_log.value),
                )
            })
            .collect();
        tree.revert_logs(L1BatchNumber(3), logs_to_revert);

        assert_eq!(tree.root_hash(), tree_metadata[3].root_hash);
        tree.save().unwrap();
    }

    // revert two more blocks
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let logs_to_revert = mirror_logs
        .iter()
        .skip(2 * block_size)
        .take(2 * block_size)
        .map(|witness_log| (witness_log.storage_log.key.hashed_key_u256(), None))
        .collect();
    {
        let mut tree = ZkSyncTree::new(storage);
        tree.revert_logs(L1BatchNumber(1), logs_to_revert);
        assert_eq!(tree.root_hash(), tree_metadata[1].root_hash);
        tree.save().unwrap();
    }

    // revert two more blocks second time
    // The result should be the same
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let logs_to_revert = mirror_logs
        .iter()
        .skip(2 * block_size)
        .take(2 * block_size)
        .map(|witness_log| (witness_log.storage_log.key.hashed_key_u256(), None))
        .collect();
    {
        let mut tree = ZkSyncTree::new(storage);
        tree.revert_logs(L1BatchNumber(1), logs_to_revert);
        assert_eq!(tree.root_hash(), tree_metadata[1].root_hash);
        tree.save().unwrap();
    }

    // reapply one of the reverted logs and verify that indexing is correct
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    {
        let storage_log = mirror_logs.get(3 * block_size).unwrap();
        let mut tree = ZkSyncTree::new(storage);
        let metadata = tree.process_block(vec![storage_log]);

        let witness_tree = TestWitnessTree::new(metadata);
        assert!(witness_tree.get_leaf((2 * block_size + 1) as u64).is_some());
        tree.save().unwrap();
    }

    // check saved block number
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let tree = ZkSyncTree::new(storage);
    assert_eq!(tree.block_number(), 3);
}

#[test]
fn reset_tree() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let storage = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let logs = gen_storage_logs();
    let mut tree = ZkSyncTree::new(storage);
    let config = TreeConfig::new(ZkHasher::default());
    let empty_tree_hash = H256::from_slice(&config.default_root_hash());

    logs.chunks(5)
        .into_iter()
        .fold(empty_tree_hash, |hash, chunk| {
            let _ = tree.process_block(chunk);
            tree.reset();
            assert_eq!(tree.root_hash(), hash);
            let _ = tree.process_block(chunk);
            tree.save().unwrap();
            tree.root_hash()
        });
}

fn convert_logs(logs: Vec<StorageLog>) -> Vec<WitnessStorageLog> {
    logs.into_iter()
        .map(|storage_log| WitnessStorageLog {
            storage_log,
            previous_value: H256::zero(),
        })
        .collect()
}

fn gen_storage_logs() -> Vec<WitnessStorageLog> {
    let addrs = vec![
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .into_iter()
    .map(|s| Address::from_str(s).unwrap());

    let proof_keys: Vec<_> = addrs
        .flat_map(|addr| {
            (0..20).map(move |i| StorageKey::new(AccountTreeId::new(addr), u32_to_h256(i)))
        })
        .collect();
    let proof_values: Vec<_> = (0..100).map(u32_to_h256).collect();

    proof_keys
        .iter()
        .zip(proof_values.iter())
        .map(|(proof_key, &proof_value)| {
            let storage_log = StorageLog::new_write_log(*proof_key, proof_value);
            WitnessStorageLog {
                storage_log,
                previous_value: H256::zero(),
            }
        })
        .collect()
}

/// This one is used only for basic test verification
pub struct TestWitnessTree {
    storage_logs: Vec<StorageLogMetadata>,
}

impl TestWitnessTree {
    pub fn new(metadata: TreeMetadata) -> Self {
        let witness_input = metadata.witness_input.unwrap();
        let storage_logs = witness_input.into_merkle_paths().collect();
        Self { storage_logs }
    }

    pub fn root(&self) -> ZkHash {
        self.storage_logs.last().unwrap().root_hash.to_vec()
    }

    pub fn get_leaf(&self, index: u64) -> Option<TreeKey> {
        for log in &self.storage_logs {
            if log.leaf_enumeration_index == index {
                return Some(log.leaf_hashed_key);
            }
        }
        None
    }
}

#[test]
fn basic_witness_workflow() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs = gen_storage_logs();
    let (first_chunk, second_chunk) = logs.split_at(logs.len() / 2);

    {
        let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
        let mut tree = ZkSyncTree::new(db);
        let metadata = tree.process_block(first_chunk);
        let witness_tree = TestWitnessTree::new(metadata);

        assert_eq!(
            witness_tree.get_leaf(1),
            Some(logs[0].storage_log.key.hashed_key_u256())
        );
        assert_eq!(
            witness_tree.get_leaf(2),
            Some(logs[1].storage_log.key.hashed_key_u256())
        );

        tree.save().unwrap();
    }
    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let mut tree = ZkSyncTree::new(db);
    let metadata = tree.process_block(second_chunk);
    let witness_tree = TestWitnessTree::new(metadata);
    assert_eq!(
        witness_tree.root(),
        [
            125, 25, 107, 171, 182, 155, 32, 70, 138, 108, 238, 150, 140, 205, 193, 39, 90, 92,
            122, 233, 118, 238, 248, 201, 160, 55, 58, 206, 244, 216, 188, 10
        ],
    );

    assert_eq!(
        witness_tree.get_leaf((logs.len() / 2 + 1) as u64),
        Some(logs[logs.len() / 2].storage_log.key.hashed_key_u256())
    );
}

#[test]
fn read_logs() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let logs: Vec<_> = gen_storage_logs().into_iter().take(5).collect();

    let write_metadata = {
        let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
        let mut tree = ZkSyncTree::new(db);
        let metadata = tree.process_block(logs.clone());
        tree.save().unwrap();
        metadata
    };

    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let mut tree = ZkSyncTree::new(db);
    let read_logs: Vec<_> = logs
        .into_iter()
        .take(5)
        .map(|log| StorageLog::new_read_log(log.storage_log.key, log.storage_log.value))
        .collect();
    let read_metadata = tree.process_block(convert_logs(read_logs));

    assert_eq!(read_metadata.root_hash, write_metadata.root_hash);
    let witness_tree = TestWitnessTree::new(read_metadata);
    assert!(witness_tree.get_leaf(1).is_some());
    assert!(witness_tree.get_leaf(2).is_some());
}

#[test]
fn root_hash_compatibility() {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let mut tree = ZkSyncTree::new(db);
    assert_eq!(
        tree.root_hash(),
        H256([
            152, 164, 142, 78, 209, 115, 97, 136, 56, 74, 232, 167, 157, 210, 28, 77, 102, 135,
            229, 253, 34, 202, 24, 20, 137, 6, 215, 135, 54, 192, 216, 106
        ]),
    );
    let storage_logs = vec![
        WitnessStorageLog {
            storage_log: StorageLog::new_write_log(
                StorageKey::new(
                    AccountTreeId::new(ACCOUNT_CODE_STORAGE_ADDRESS),
                    H256::zero(),
                ),
                [1u8; 32].into(),
            ),
            previous_value: H256::zero(),
        },
        WitnessStorageLog {
            storage_log: StorageLog::new_write_log(
                StorageKey::new(
                    AccountTreeId::new(Address::from_low_u64_be(9223372036854775808)),
                    H256::from(&[254; 32]),
                ),
                [
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 254,
                ]
                .into(),
            ),
            previous_value: H256::zero(),
        },
        WitnessStorageLog {
            storage_log: StorageLog::new_write_log(
                StorageKey::new(
                    AccountTreeId::new(Address::from_low_u64_be(9223372036854775809)),
                    H256::from(&[253; 32]),
                ),
                [
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253,
                ]
                .into(),
            ),
            previous_value: H256::zero(),
        },
        WitnessStorageLog {
            storage_log: StorageLog::new_write_log(
                StorageKey::new(
                    AccountTreeId::new(Address::from_low_u64_be(9223372036854775810)),
                    H256::from(&[252; 32]),
                ),
                [
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 252,
                ]
                .into(),
            ),
            previous_value: H256::zero(),
        },
        WitnessStorageLog {
            storage_log: StorageLog::new_write_log(
                StorageKey::new(
                    AccountTreeId::new(Address::from_low_u64_be(9223372036854775811)),
                    H256::from(&[251; 32]),
                ),
                [
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 251,
                ]
                .into(),
            ),
            previous_value: H256::zero(),
        },
        WitnessStorageLog {
            storage_log: StorageLog::new_write_log(
                StorageKey::new(
                    AccountTreeId::new(Address::from_low_u64_be(9223372036854775812)),
                    H256::from(&[250; 32]),
                ),
                [
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 250,
                ]
                .into(),
            ),
            previous_value: H256::zero(),
        },
    ];
    let metadata = tree.process_block(storage_logs);
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
    let rocks_db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let mut tree = ZkSyncTree::new(rocks_db);
    let tree_metadata = tree.process_block(gen_storage_logs());

    // simulate server restart by calling process_block again on the same tree.

    let tree_metadata_second = tree.process_block(gen_storage_logs());
    assert_eq!(
        tree_metadata.initial_writes, tree_metadata_second.initial_writes,
        "initial writes must be same on multiple calls to process_block to ensure idempotency"
    );
    assert_eq!(
        tree_metadata.repeated_writes, tree_metadata_second.repeated_writes,
        "repeated writes must be same on multiple calls to process_block to ensure idempotency"
    );
}
