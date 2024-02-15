use zksync_protobuf::repr::{decode, encode};
use zksync_types::{ExecuteTransactionCommon, Transaction};

use crate::tests::{mock_l1_execute, mock_l2_transaction, mock_protocol_upgrade_transaction};

/// Tests struct <-> proto struct conversions.
#[test]
fn test_encoding() {
    encode_decode(mock_l1_execute().into());
    encode_decode(mock_l2_transaction().into());
    encode_decode(mock_protocol_upgrade_transaction().into());
}

fn encode_decode(tx1: Transaction) {
    let bytes = encode::<super::proto::Transaction>(&tx1);
    let tx2 = decode::<super::proto::Transaction>(&bytes).unwrap();
    assert_transaction_eq(&tx1, &tx2);
}

/// Checks equality. Top-level `PartialEq` of `Transaction` checks only
/// the hash hence it's not usable here.
fn assert_transaction_eq(tx1: &Transaction, tx2: &Transaction) {
    assert_eq!(tx1.received_timestamp_ms, tx2.received_timestamp_ms);
    assert_eq!(tx1.raw_bytes, tx2.raw_bytes);
    assert_eq!(tx1.execute, tx2.execute);
    match (&tx1.common_data, &tx2.common_data) {
        (
            ExecuteTransactionCommon::L1(tx1_common_data),
            ExecuteTransactionCommon::L1(tx2_common_data),
        ) => {
            assert_eq!(tx1_common_data.sender, tx2_common_data.sender);
            assert_eq!(tx1_common_data.serial_id, tx2_common_data.serial_id);
            assert_eq!(
                tx1_common_data.deadline_block,
                tx2_common_data.deadline_block
            );
            assert_eq!(
                tx1_common_data.layer_2_tip_fee,
                tx2_common_data.layer_2_tip_fee
            );
            assert_eq!(tx1_common_data.full_fee, tx2_common_data.full_fee);
            assert_eq!(
                tx1_common_data.max_fee_per_gas,
                tx2_common_data.max_fee_per_gas
            );
            assert_eq!(tx1_common_data.gas_limit, tx2_common_data.gas_limit);
            assert_eq!(
                tx1_common_data.gas_per_pubdata_limit,
                tx2_common_data.gas_per_pubdata_limit
            );
            assert_eq!(
                tx1_common_data.priority_queue_type,
                tx2_common_data.priority_queue_type
            );
            assert_eq!(tx1_common_data.eth_hash, tx2_common_data.eth_hash);
            assert_eq!(tx1_common_data.eth_block, tx2_common_data.eth_block);
            assert_eq!(
                tx1_common_data.canonical_tx_hash,
                tx2_common_data.canonical_tx_hash
            );
            assert_eq!(tx1_common_data.to_mint, tx2_common_data.to_mint);
            assert_eq!(
                tx1_common_data.refund_recipient,
                tx2_common_data.refund_recipient
            );
        }
        (
            ExecuteTransactionCommon::L2(tx1_common_data),
            ExecuteTransactionCommon::L2(tx2_common_data),
        ) => {
            assert_eq!(tx1_common_data, tx2_common_data);
        }
        (
            ExecuteTransactionCommon::ProtocolUpgrade(tx1_common_data),
            ExecuteTransactionCommon::ProtocolUpgrade(tx2_common_data),
        ) => {
            assert_eq!(tx1_common_data.sender, tx2_common_data.sender);
            assert_eq!(tx1_common_data.upgrade_id, tx2_common_data.upgrade_id);
            assert_eq!(
                tx1_common_data.max_fee_per_gas,
                tx2_common_data.max_fee_per_gas
            );
            assert_eq!(tx1_common_data.gas_limit, tx2_common_data.gas_limit);
            assert_eq!(
                tx1_common_data.gas_per_pubdata_limit,
                tx2_common_data.gas_per_pubdata_limit
            );
            assert_eq!(tx1_common_data.eth_hash, tx2_common_data.eth_hash);
            assert_eq!(tx1_common_data.eth_block, tx2_common_data.eth_block);
            assert_eq!(
                tx1_common_data.canonical_tx_hash,
                tx2_common_data.canonical_tx_hash
            );
            assert_eq!(tx1_common_data.to_mint, tx2_common_data.to_mint);
            assert_eq!(
                tx1_common_data.refund_recipient,
                tx2_common_data.refund_recipient
            );
        }
        (_, _) => panic!(
            "common_data variant mismatch:\n{}\n{}",
            tx1.common_data, tx2.common_data
        ),
    }
}
