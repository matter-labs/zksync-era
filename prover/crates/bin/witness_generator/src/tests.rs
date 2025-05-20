use std::iter;

use const_decoder::Decoder::Hex;
use zkevm_test_harness::{
    kzg::KzgSettings,
    witness::tree::{BinarySparseStorageTree, ZkSyncStorageLeaf},
};
use zksync_prover_interface::inputs::{StorageLogMetadata, WitnessInputMerklePaths};
use zksync_types::U256;

use super::precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider;
use crate::utils::KZG_TRUSTED_SETUP_FILE;

// Sample `StorageLogMetadata` entries. Since we cannot allocate in constants, we store
// the only Merkle path hash separately.
const LOGS_AND_PATHS: [(StorageLogMetadata, [u8; 32]); 3] = [
    generate_storage_log_metadata(
        b"DDC60818D8F7CFE42514F8EA3CC52806DDC60818D8F7CFE42514F8EA3CC52806",
        b"12E9FF974B0FAEE514AD4AC50E2BDC6E12E9FF974B0FAEE514AD4AC50E2BDC6E",
        false,
        false,
        1,
    ),
    generate_storage_log_metadata(
        b"BDA1617CC883E2251D3BE0FD9B3F3064BDA1617CC883E2251D3BE0FD9B3F3064",
        b"D14917FCB067922F92322025D1BA50B4D14917FCB067922F92322025D1BA50B4",
        true,
        true,
        2,
    ),
    generate_storage_log_metadata(
        b"77F035AD50811CFABD956F6F1B48E48277F035AD50811CFABD956F6F1B48E482",
        b"7CF33B959916CC9B56F21C427ED7CA187CF33B959916CC9B56F21C427ED7CA18",
        true,
        true,
        3,
    ),
];

const LEAFS: [ZkSyncStorageLeaf; 2] = [
    generate_leaf(
        1,
        b"AD558076F725ED8B5E5B42920422E9BEAD558076F725ED8B5E5B42920422E9BE",
    ),
    generate_leaf(
        1,
        b"98A0EADBD6118391B744252DA348873C98A0EADBD6118391B744252DA348873C",
    ),
];

const INDICES: [[u8; 32]; 2] = [
    Hex.decode(b"5534D106E0B590953AC0FC7D65CA3B2E5534D106E0B590953AC0FC7D65CA3B2E"),
    Hex.decode(b"00309D72EF0AD9786DA9044109E1704B00309D72EF0AD9786DA9044109E1704B"),
];

const fn generate_leaf(index: u64, value: &[u8]) -> ZkSyncStorageLeaf {
    ZkSyncStorageLeaf {
        index,
        value: Hex.decode(value),
    }
}

const fn generate_storage_log_metadata(
    root_hash: &[u8],
    merkle_path: &[u8],
    is_write: bool,
    first_write: bool,
    leaf_enumeration_index: u64,
) -> (StorageLogMetadata, [u8; 32]) {
    let log = StorageLogMetadata {
        root_hash: Hex.decode(root_hash),
        is_write,
        first_write,
        merkle_paths: Vec::new(),
        leaf_hashed_key: U256::zero(),
        leaf_enumeration_index,
        value_written: [0; 32],
        value_read: [0; 32],
    };
    (log, Hex.decode(merkle_path))
}

fn create_provider() -> PrecalculatedMerklePathsProvider {
    let mut job = WitnessInputMerklePaths::new(4);
    for (mut log, merkle_path) in LOGS_AND_PATHS {
        log.merkle_paths = vec![merkle_path];
        job.push_merkle_path(log);
    }
    PrecalculatedMerklePathsProvider::new(job, [0_u8; 32])
}

#[test]
fn test_filter_renumerate_all_first_writes() {
    let mut provider = create_provider();
    for log in &mut provider.pending_leaves {
        log.first_write = true;
    }

    let (_, first_writes, updates) =
        provider.filter_renumerate(INDICES.iter(), LEAFS.iter().copied());
    assert_eq!(2, first_writes.len());
    let (first_write_index, first_write_leaf) = first_writes[0];
    assert_eq!(first_write_index, INDICES[0]);
    assert_eq!(first_write_leaf.value, LEAFS[0].value);
    assert_eq!(0, updates.len());
}

#[test]
fn test_filter_renumerate_all_repeated_writes() {
    let mut provider = create_provider();
    for log in &mut provider.pending_leaves {
        log.first_write = false;
    }

    let (_, first_writes, updates) =
        provider.filter_renumerate(INDICES.iter(), LEAFS.iter().copied());
    assert_eq!(0, first_writes.len());
    assert_eq!(2, updates.len());
    assert_eq!(updates[0].value, LEAFS[0].value);
    assert_eq!(updates[1].value, LEAFS[1].value);
}

#[test]
fn test_filter_renumerate_repeated_writes_with_first_write() {
    let mut provider = create_provider();
    for (i, log) in provider.pending_leaves.iter_mut().enumerate() {
        log.first_write = i == 2;
    }

    let (_, first_writes, updates) =
        provider.filter_renumerate(INDICES.iter(), LEAFS.iter().copied());
    assert_eq!(1, first_writes.len());
    assert_eq!(1, updates.len());
    assert_eq!(3, first_writes[0].1.index);
    assert_eq!(2, updates[0].index);
}

#[test]
#[should_panic(expected = "leafs must be of same length as indexes")]
fn test_filter_renumerate_panic_when_leafs_and_indices_are_of_different_length() {
    const ANOTHER_LEAF: ZkSyncStorageLeaf = generate_leaf(
        2,
        b"72868932BBB002043AF50363EEB65AE172868932BBB002043AF50363EEB65AE1",
    );

    let provider = create_provider();
    let leafs = LEAFS.iter().copied().chain([ANOTHER_LEAF]);
    provider.filter_renumerate(INDICES.iter(), leafs);
}

#[test]
#[should_panic(expected = "indexes must be of same length as leafs and pending leaves")]
fn test_filter_renumerate_panic_when_indices_and_pending_leaves_are_of_different_length() {
    const ANOTHER_INDEX: [u8; 32] =
        Hex.decode(b"930058748339A83E06F0D1D22937E92A930058748339A83E06F0D1D22937E92A");

    let provider = create_provider();
    let indices = INDICES.iter().chain([&ANOTHER_INDEX]);
    provider.filter_renumerate(indices, LEAFS.iter().copied());
}

#[test]
fn vec_and_vec_deque_serializations_are_compatible() {
    let logs = create_provider().pending_leaves;
    let serialized = bincode::serialize(&logs).unwrap();
    let logs_vec: Vec<StorageLogMetadata> = bincode::deserialize(&serialized).unwrap();
    assert_eq!(logs, logs_vec);
    let serialized_deque = bincode::serialize(&logs_vec).unwrap();
    assert_eq!(serialized_deque, serialized);
}

#[test]
fn provider_serialization() {
    let provider = create_provider();

    let serialized = bincode::serialize(&provider).unwrap();
    // Check that logs are serialized in the natural order.
    let needle = LOGS_AND_PATHS[0].0.root_hash;
    let mut windows = serialized.windows(needle.len());
    windows.position(|window| *window == needle).unwrap();
    let needle = LOGS_AND_PATHS[1].0.root_hash;
    windows.position(|window| *window == needle).unwrap();
    let needle = LOGS_AND_PATHS[2].0.root_hash;
    windows.position(|window| *window == needle).unwrap();

    let deserialized: PrecalculatedMerklePathsProvider = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, provider);
}

#[test]
fn initializing_provider_with_compacted_merkle_paths() {
    let mut provider = create_provider();
    for log in &mut provider.pending_leaves {
        let empty_merkle_paths = iter::repeat_n([0; 32], 255);
        log.merkle_paths.splice(0..0, empty_merkle_paths);
    }

    // First log entry: read
    let query = provider.get_leaf(&[0; 32]);
    assert!(!query.first_write);
    assert!(query.merkle_path[0..255]
        .iter()
        .all(|hash| *hash == [0; 32]));

    // Second log entry: first write
    let query = provider.get_leaf(&[0; 32]);
    assert!(query.first_write);
    assert!(query.merkle_path[0..255]
        .iter()
        .all(|hash| *hash == [0; 32]));
    assert_ne!(query.merkle_path[255], [0; 32]);

    let query = provider.insert_leaf(
        &[0; 32],
        ZkSyncStorageLeaf {
            index: 2,
            value: [0; 32],
        },
    );
    assert!(query.first_write);
    assert!(query.merkle_path[0..255]
        .iter()
        .all(|hash| *hash == [0; 32]));
    assert_ne!(query.merkle_path[255], [0; 32]);
}

#[test]
fn trusted_setup_can_be_read() {
    let trusted_setup_path = KZG_TRUSTED_SETUP_FILE
        .path()
        .to_str()
        .expect("Path to KZG trusted setup is not a UTF-8 string");
    KzgSettings::new(trusted_setup_path);
}
