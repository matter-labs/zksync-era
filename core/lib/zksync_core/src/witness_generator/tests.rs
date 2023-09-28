use crate::witness_generator::precalculated_merkle_paths_provider::PrecalculatedMerklePathsProvider;
use std::convert::TryInto;
use zksync_types::proofs::StorageLogMetadata;
use zksync_types::zkevm_test_harness::witness::tree::{BinarySparseStorageTree, ZkSyncStorageLeaf};

#[test]
fn test_filter_renumerate_all_first_writes() {
    let logs = vec![
        generate_storage_log_metadata(
            "DDC60818D8F7CFE42514F8EA3CC52806DDC60818D8F7CFE42514F8EA3CC52806",
            "12E9FF974B0FAEE514AD4AC50E2BDC6E12E9FF974B0FAEE514AD4AC50E2BDC6E",
            false,
            false,
            1,
        ),
        generate_storage_log_metadata(
            "BDA1617CC883E2251D3BE0FD9B3F3064BDA1617CC883E2251D3BE0FD9B3F3064",
            "D14917FCB067922F92322025D1BA50B4D14917FCB067922F92322025D1BA50B4",
            true,
            true,
            2,
        ),
        generate_storage_log_metadata(
            "77F035AD50811CFABD956F6F1B48E48277F035AD50811CFABD956F6F1B48E482",
            "7CF33B959916CC9B56F21C427ED7CA187CF33B959916CC9B56F21C427ED7CA18",
            true,
            true,
            3,
        ),
    ];
    let precalculated_merkle_paths_provider = PrecalculatedMerklePathsProvider {
        root_hash: string_to_array(
            "4AF44B3D5D4F9C7B117A68351AAB65CF4AF44B3D5D4F9C7B117A68351AAB65CF",
        ),
        pending_leaves: logs,
        next_enumeration_index: 4,
        is_get_leaf_invoked: false,
    };
    let (leafs, indices) = generate_leafs_indices();

    let (_, first_writes, updates) =
        precalculated_merkle_paths_provider.filter_renumerate(indices.iter(), leafs.into_iter());
    assert_eq!(2, first_writes.len());
    assert_eq!(0, updates.len());
}

#[test]
fn test_filter_renumerate_all_repeated_writes() {
    let logs = vec![
        generate_storage_log_metadata(
            "DDC60818D8F7CFE42514F8EA3CC52806DDC60818D8F7CFE42514F8EA3CC52806",
            "12E9FF974B0FAEE514AD4AC50E2BDC6E12E9FF974B0FAEE514AD4AC50E2BDC6E",
            false,
            false,
            1,
        ),
        generate_storage_log_metadata(
            "BDA1617CC883E2251D3BE0FD9B3F3064BDA1617CC883E2251D3BE0FD9B3F3064",
            "D14917FCB067922F92322025D1BA50B4D14917FCB067922F92322025D1BA50B4",
            true,
            false,
            2,
        ),
        generate_storage_log_metadata(
            "77F035AD50811CFABD956F6F1B48E48277F035AD50811CFABD956F6F1B48E482",
            "7CF33B959916CC9B56F21C427ED7CA187CF33B959916CC9B56F21C427ED7CA18",
            true,
            false,
            3,
        ),
    ];
    let precalculated_merkle_paths_provider = PrecalculatedMerklePathsProvider {
        root_hash: string_to_array(
            "4AF44B3D5D4F9C7B117A68351AAB65CF4AF44B3D5D4F9C7B117A68351AAB65CF",
        ),
        pending_leaves: logs,
        next_enumeration_index: 4,
        is_get_leaf_invoked: false,
    };
    let (leafs, indices) = generate_leafs_indices();

    let (_, first_writes, updates) =
        precalculated_merkle_paths_provider.filter_renumerate(indices.iter(), leafs.into_iter());
    assert_eq!(0, first_writes.len());
    assert_eq!(2, updates.len());
}

#[test]
fn test_filter_renumerate_repeated_writes_with_first_write() {
    let logs = vec![
        generate_storage_log_metadata(
            "DDC60818D8F7CFE42514F8EA3CC52806DDC60818D8F7CFE42514F8EA3CC52806",
            "12E9FF974B0FAEE514AD4AC50E2BDC6E12E9FF974B0FAEE514AD4AC50E2BDC6E",
            false,
            false,
            1,
        ),
        generate_storage_log_metadata(
            "BDA1617CC883E2251D3BE0FD9B3F3064BDA1617CC883E2251D3BE0FD9B3F3064",
            "D14917FCB067922F92322025D1BA50B4D14917FCB067922F92322025D1BA50B4",
            true,
            false,
            2,
        ),
        generate_storage_log_metadata(
            "77F035AD50811CFABD956F6F1B48E48277F035AD50811CFABD956F6F1B48E482",
            "7CF33B959916CC9B56F21C427ED7CA187CF33B959916CC9B56F21C427ED7CA18",
            true,
            true,
            3,
        ),
    ];
    let precalculated_merkle_paths_provider = PrecalculatedMerklePathsProvider {
        root_hash: string_to_array(
            "4AF44B3D5D4F9C7B117A68351AAB65CF4AF44B3D5D4F9C7B117A68351AAB65CF",
        ),
        pending_leaves: logs,
        next_enumeration_index: 4,
        is_get_leaf_invoked: false,
    };
    let (leafs, indices) = generate_leafs_indices();

    let (_, first_writes, updates) =
        precalculated_merkle_paths_provider.filter_renumerate(indices.iter(), leafs.into_iter());
    assert_eq!(1, first_writes.len());
    assert_eq!(1, updates.len());
    assert_eq!(3, first_writes[0].1.index);
    assert_eq!(2, updates[0].index);
}

#[test]
#[should_panic(expected = "leafs must be of same length as indexes")]
fn test_filter_renumerate_panic_when_leafs_and_indices_are_of_different_length() {
    let logs = vec![
        generate_storage_log_metadata(
            "DDC60818D8F7CFE42514F8EA3CC52806DDC60818D8F7CFE42514F8EA3CC52806",
            "12E9FF974B0FAEE514AD4AC50E2BDC6E12E9FF974B0FAEE514AD4AC50E2BDC6E",
            false,
            false,
            1,
        ),
        generate_storage_log_metadata(
            "BDA1617CC883E2251D3BE0FD9B3F3064BDA1617CC883E2251D3BE0FD9B3F3064",
            "D14917FCB067922F92322025D1BA50B4D14917FCB067922F92322025D1BA50B4",
            true,
            false,
            2,
        ),
        generate_storage_log_metadata(
            "77F035AD50811CFABD956F6F1B48E48277F035AD50811CFABD956F6F1B48E482",
            "7CF33B959916CC9B56F21C427ED7CA187CF33B959916CC9B56F21C427ED7CA18",
            true,
            true,
            3,
        ),
    ];
    let precalculated_merkle_paths_provider = PrecalculatedMerklePathsProvider {
        root_hash: string_to_array(
            "4AF44B3D5D4F9C7B117A68351AAB65CF4AF44B3D5D4F9C7B117A68351AAB65CF",
        ),
        pending_leaves: logs,
        next_enumeration_index: 4,
        is_get_leaf_invoked: false,
    };

    let leafs = vec![
        generate_leaf(
            1,
            "AD558076F725ED8B5E5B42920422E9BEAD558076F725ED8B5E5B42920422E9BE",
        ),
        generate_leaf(
            1,
            "98A0EADBD6118391B744252DA348873C98A0EADBD6118391B744252DA348873C",
        ),
        generate_leaf(
            2,
            "72868932BBB002043AF50363EEB65AE172868932BBB002043AF50363EEB65AE1",
        ),
    ];
    let indices = [
        string_to_array("5534D106E0B590953AC0FC7D65CA3B2E5534D106E0B590953AC0FC7D65CA3B2E"),
        string_to_array("00309D72EF0AD9786DA9044109E1704B00309D72EF0AD9786DA9044109E1704B"),
    ];

    precalculated_merkle_paths_provider.filter_renumerate(indices.iter(), leafs.into_iter());
}

#[test]
#[should_panic(expected = "indexes must be of same length as leafs and pending leaves")]
fn test_filter_renumerate_panic_when_indices_and_pending_leaves_are_of_different_length() {
    let logs = vec![
        generate_storage_log_metadata(
            "DDC60818D8F7CFE42514F8EA3CC52806DDC60818D8F7CFE42514F8EA3CC52806",
            "12E9FF974B0FAEE514AD4AC50E2BDC6E12E9FF974B0FAEE514AD4AC50E2BDC6E",
            false,
            false,
            1,
        ),
        generate_storage_log_metadata(
            "BDA1617CC883E2251D3BE0FD9B3F3064BDA1617CC883E2251D3BE0FD9B3F3064",
            "D14917FCB067922F92322025D1BA50B4D14917FCB067922F92322025D1BA50B4",
            true,
            false,
            2,
        ),
        generate_storage_log_metadata(
            "77F035AD50811CFABD956F6F1B48E48277F035AD50811CFABD956F6F1B48E482",
            "7CF33B959916CC9B56F21C427ED7CA187CF33B959916CC9B56F21C427ED7CA18",
            true,
            true,
            3,
        ),
    ];
    let precalculated_merkle_paths_provider = PrecalculatedMerklePathsProvider {
        root_hash: string_to_array(
            "4AF44B3D5D4F9C7B117A68351AAB65CF4AF44B3D5D4F9C7B117A68351AAB65CF",
        ),
        pending_leaves: logs,
        next_enumeration_index: 4,
        is_get_leaf_invoked: false,
    };

    let leafs = vec![
        generate_leaf(
            1,
            "AD558076F725ED8B5E5B42920422E9BEAD558076F725ED8B5E5B42920422E9BE",
        ),
        generate_leaf(
            1,
            "98A0EADBD6118391B744252DA348873C98A0EADBD6118391B744252DA348873C",
        ),
        generate_leaf(
            2,
            "72868932BBB002043AF50363EEB65AE172868932BBB002043AF50363EEB65AE1",
        ),
    ];
    let indices = [
        string_to_array("5534D106E0B590953AC0FC7D65CA3B2E5534D106E0B590953AC0FC7D65CA3B2E"),
        string_to_array("00309D72EF0AD9786DA9044109E1704B00309D72EF0AD9786DA9044109E1704B"),
        string_to_array("930058748339A83E06F0D1D22937E92A930058748339A83E06F0D1D22937E92A"),
    ];

    precalculated_merkle_paths_provider.filter_renumerate(indices.iter(), leafs.into_iter());
}

fn generate_leafs_indices() -> (Vec<ZkSyncStorageLeaf>, Vec<[u8; 32]>) {
    let leafs = vec![
        generate_leaf(
            1,
            "AD558076F725ED8B5E5B42920422E9BEAD558076F725ED8B5E5B42920422E9BE",
        ),
        generate_leaf(
            2,
            "72868932BBB002043AF50363EEB65AE172868932BBB002043AF50363EEB65AE1",
        ),
    ];
    let indices = vec![
        string_to_array("5534D106E0B590953AC0FC7D65CA3B2E5534D106E0B590953AC0FC7D65CA3B2E"),
        string_to_array("00309D72EF0AD9786DA9044109E1704B00309D72EF0AD9786DA9044109E1704B"),
    ];
    (leafs, indices)
}

fn generate_leaf(index: u64, value: &str) -> ZkSyncStorageLeaf {
    ZkSyncStorageLeaf {
        index,
        value: string_to_array(value),
    }
}

fn string_to_array(value: &str) -> [u8; 32] {
    let array_value: [u8; 32] = hex::decode(value)
        .expect("Hex decoding failed")
        .try_into()
        .unwrap();
    array_value
}

fn generate_storage_log_metadata(
    root_hash: &str,
    merkle_path: &str,
    is_write: bool,
    first_write: bool,
    leaf_enumeration_index: u64,
) -> StorageLogMetadata {
    StorageLogMetadata {
        root_hash: string_to_array(root_hash),
        is_write,
        first_write,
        merkle_paths: vec![string_to_array(merkle_path)],
        leaf_hashed_key: Default::default(),
        leaf_enumeration_index,
        value_written: [0; 32],
        value_read: [0; 32],
    }
}
