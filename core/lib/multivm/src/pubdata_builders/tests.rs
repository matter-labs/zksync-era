use zksync_types::{
    commitment::{L2DACommitmentScheme, L2PubdataValidator},
    u256_to_h256,
    writes::StateDiffRecord,
    Address, ProtocolVersionId, ACCOUNT_CODE_STORAGE_ADDRESS, BOOTLOADER_ADDRESS,
};

use super::{full_builder::FullPubdataBuilder, hashed_builder::HashedPubdataBuilder};
use crate::interface::pubdata::{L1MessengerL2ToL1Log, PubdataBuilder, PubdataInput};

fn mock_input() -> PubdataInput {
    // Just using some constant addresses for tests
    let addr1 = BOOTLOADER_ADDRESS;
    let addr2 = ACCOUNT_CODE_STORAGE_ADDRESS;

    let user_logs = vec![L1MessengerL2ToL1Log {
        l2_shard_id: 0,
        is_service: false,
        tx_number_in_block: 0,
        sender: addr1,
        key: 1.into(),
        value: 128.into(),
    }];

    let l2_to_l1_messages = vec![hex::decode("deadbeef").unwrap()];

    let published_bytecodes = vec![hex::decode("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap()];

    // For covering more cases, we have two state diffs:
    // One with enumeration index present (and so it is a repeated write) and the one without it.
    let state_diffs = vec![
        StateDiffRecord {
            address: addr2,
            key: 155.into(),
            derived_key: u256_to_h256(125.into()).0,
            enumeration_index: 12,
            initial_value: 11.into(),
            final_value: 12.into(),
        },
        StateDiffRecord {
            address: addr2,
            key: 156.into(),
            derived_key: u256_to_h256(126.into()).0,
            enumeration_index: 0,
            initial_value: 0.into(),
            final_value: 14.into(),
        },
    ];

    PubdataInput {
        user_logs,
        l2_to_l1_messages,
        published_bytecodes,
        state_diffs,
    }
}

#[test]
fn test_full_pubdata_building() {
    let input = mock_input();

    let full_pubdata_builder =
        FullPubdataBuilder::new(L2PubdataValidator::Address(Address::zero()));

    let actual =
        full_pubdata_builder.l1_messenger_operator_input(&input, ProtocolVersionId::Version24);
    let expected = "00000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000060bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901000000020000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009b000000000000000000000000000000000000000000000000000000000000007d000000000000000c000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009c000000000000000000000000000000000000000000000000000000000000007e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `l1_messenger_operator_input` (pre gateway)"
    );

    let actual =
        full_pubdata_builder.settlement_layer_pubdata(&input, ProtocolVersionId::Version24);
    let expected = "00000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000060bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `settlement_layer_pubdata` (pre gateway)"
    );

    let actual =
        full_pubdata_builder.l1_messenger_operator_input(&input, ProtocolVersionId::Version27);
    let expected = "89f9a07233e608561d90f7c4e7bcea24d718e425a6bd6c8eefb48a334366143694c75fae278944d856d68e33bbd32937cb3a1ea35cbf7d6eeeb1150f500dd0d64d0efe420d6dafe5897eab2fc27b2e47af303397ed285ace146d836d042717b0a3dc4b28a603a33b28ce1d5c52c593a46a15a99f1afa1c1d92715284288958fd54a93de700000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000032300000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000060bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901000000020000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009b000000000000000000000000000000000000000000000000000000000000007d000000000000000c000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008002000000000000000000000000000000000000000000000000000000000000009c000000000000000000000000000000000000000000000000000000000000007e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `l1_messenger_operator_input` (post gateway)"
    );

    let actual =
        full_pubdata_builder.settlement_layer_pubdata(&input, ProtocolVersionId::Version27);
    let expected = "00000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000060bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `settlement_layer_pubdata` (post gateway)"
    );

    let full_pubdata_builder = FullPubdataBuilder::new(L2PubdataValidator::CommitmentScheme(
        L2DACommitmentScheme::BlobsAndPubdataKeccak256,
    ));

    let actual =
        full_pubdata_builder.settlement_layer_pubdata(&input, ProtocolVersionId::Version31);
    let expected = "00000001000000000000000000000000000000000000000000008001000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000100000004deadbeef0000000100000060bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0100002a040001000000000000000000000000000000000000000000000000000000000000007e090e0000000c0901";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `BlobsAndPubdataKeccak256` (post gateway)"
    );
}

#[test]
fn test_hashed_pubdata_building() {
    let input = mock_input();

    let hashed_pubdata_builder =
        HashedPubdataBuilder::new(L2PubdataValidator::Address(Address::zero()));

    let actual =
        hashed_pubdata_builder.l1_messenger_operator_input(&input, ProtocolVersionId::Version27);
    let expected = "89f9a07233e608561d90f7c4e7bcea24d718e425a6bd6c8eefb48a334366143694c75fae278944d856d68e33bbd32937cb3a1ea35cbf7d6eeeb1150f500dd0d64d0efe420d6dafe5897eab2fc27b2e47af303397ed285ace146d836d042717b0a3dc4b28a603a33b28ce1d5c52c593a46a15a99f1afa1c1d92715284288958fd54a93de700000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000005c000000010000000000000000000000000000000000000000000080010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000008000000000";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `l1_messenger_operator_input`"
    );

    let actual =
        hashed_pubdata_builder.settlement_layer_pubdata(&input, ProtocolVersionId::Version27);
    let expected = "fa96e2436e6fb4d668f5a06681a7c53fcb199b2747ee624ee52a13e85aac5f1e";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `settlement_layer_pubdata`"
    );

    let hashed_pubdata_builder = HashedPubdataBuilder::new(L2PubdataValidator::CommitmentScheme(
        L2DACommitmentScheme::BlobsAndPubdataKeccak256,
    ));
    let actual =
        hashed_pubdata_builder.settlement_layer_pubdata(&input, ProtocolVersionId::Version31);
    let expected = "fa96e2436e6fb4d668f5a06681a7c53fcb199b2747ee624ee52a13e85aac5f1e";
    assert_eq!(
        &hex::encode(actual),
        expected,
        "mismatch for `settlement_layer_pubdata`"
    );
}
