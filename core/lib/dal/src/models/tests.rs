use chrono::Utc;
use zksync_types::{
    fee::Fee,
    l1::{OpProcessingType, PriorityQueueType},
    l2::TransactionType,
    web3::Bytes,
    Address, Execute, ExecuteTransactionCommon, Transaction, EIP_1559_TX_TYPE, EIP_2930_TX_TYPE,
    EIP_712_TX_TYPE, H160, H256, PRIORITY_OPERATION_L2_TX_TYPE, PROTOCOL_UPGRADE_TX_TYPE, U256,
};
use zksync_utils::bigdecimal_to_u256;

use crate::{models::storage_transaction::StorageTransaction, BigDecimal};

fn default_execute() -> Execute {
    Execute {
        contract_address: Some(H160::random()),
        value: U256::from(10i32),
        calldata: hex::decode(
            "a9059cbb00000000000000000000000058d595f318167d5af45d9e44ade4348dd4e\
    8cdfd0000000000000000000000000000000000000000000000000000000157d600d0",
        )
        .unwrap(),
        factory_deps: vec![],
    }
}

/// Used for storage transaction to protocol upgrade transaction tests
fn protocol_upgrade_storage_tx() -> StorageTransaction {
    StorageTransaction {
        hash: H256::random().as_bytes().to_vec(),
        data: serde_json::to_value(default_execute().clone()).expect("invalid value"),
        received_at: Utc::now().naive_utc(),
        tx_format: Some(PROTOCOL_UPGRADE_TX_TYPE.into()),
        gas_limit: Some(BigDecimal::from(999)),
        l1_tx_mint: Some(BigDecimal::from(666)),
        l1_tx_refund_recipient: Some(Address::random().as_bytes().to_vec()),
        initiator_address: Address::random().as_bytes().to_vec(),
        upgrade_id: Some(1i32),
        max_fee_per_gas: Some(BigDecimal::from(555)),
        gas_per_pubdata_limit: Some(BigDecimal::from(444)),
        l1_block_number: Some(1),
        ..StorageTransaction::default()
    }
}

/// Used for storage transaction to l1 transaction tests
fn l1_storage_tx() -> StorageTransaction {
    StorageTransaction {
        gas_limit: Some(BigDecimal::from(999)),
        full_fee: Some(BigDecimal::from(888)),
        layer_2_tip_fee: Some(BigDecimal::from(777)),
        l1_tx_mint: Some(BigDecimal::from(666)),
        l1_tx_refund_recipient: Some(Address::random().as_bytes().to_vec()),
        hash: H256::random().as_bytes().to_vec(),
        initiator_address: Address::random().as_bytes().to_vec(),
        priority_op_id: Some(1),
        max_fee_per_gas: Some(BigDecimal::from(555)),
        gas_per_pubdata_limit: Some(BigDecimal::from(444)),
        l1_block_number: Some(1),
        data: serde_json::to_value(default_execute().clone()).expect("invalid value"),
        received_at: Utc::now().naive_utc(),
        tx_format: Some(PRIORITY_OPERATION_L2_TX_TYPE.into()),
        ..StorageTransaction::default()
    }
}

/// Used for storage transaction to layer 2 transaction tests
fn l2_storage_tx(tx_format: i32) -> StorageTransaction {
    StorageTransaction {
        hash: H256::random().as_bytes().to_vec(),
        data: serde_json::to_value(default_execute().clone()).expect("invalid value"),
        received_at: Utc::now().naive_utc(),
        tx_format: Some(tx_format),
        nonce: Some(11),
        max_fee_per_gas: Some(BigDecimal::from(555)),
        max_priority_fee_per_gas: Some(BigDecimal::from(666)),
        gas_per_pubdata_limit: Some(BigDecimal::from(444)),
        gas_limit: Some(BigDecimal::from(999)),
        paymaster: Address::random().as_bytes().to_vec(),
        initiator_address: Address::random().as_bytes().to_vec(),
        input: Some(
            hex::decode(
                "a9059cbb0000000000000000000000003d96c32a3\
    c7c445d188805825eb57aab4e0bfa90000000000000000000000000000000000000000000000000000000001dcd6500",
            )
            .unwrap(),
        ),
        paymaster_input: hex::decode(
            "a9059cbb00000000000000000000000058d595f318167d5\
    af45d9e44ade4348dd4e8cdfd0000000000000000000000000000000000000000000000000000000157d600d0",
        )
        .unwrap(),
        // correctness of the signature doesn't matter for this test
        signature: Some("ABCD".as_bytes().to_vec()),
        ..StorageTransaction::default()
    }
}

#[test]
fn storage_tx_to_l1_tx() {
    let stx = l1_storage_tx();
    let tx = Transaction::from(stx.clone());

    let execute: Execute = serde_json::from_value(stx.data.clone()).unwrap();
    assert_eq!(execute, tx.execute);
    assert_ne!(0, tx.received_timestamp_ms);
    assert_eq!(None, tx.raw_bytes);

    if let ExecuteTransactionCommon::L1(l1_data) = tx.common_data {
        assert_eq!(
            stx.full_fee.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.full_fee
        );
        assert_eq!(
            stx.layer_2_tip_fee.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.layer_2_tip_fee
        );
        assert_eq!(PriorityQueueType::Deque, l1_data.priority_queue_type);
        assert_eq!(OpProcessingType::Common, l1_data.op_processing_type);
        assert_eq!(
            Address::from_slice(stx.initiator_address.as_slice()),
            l1_data.sender
        );
        assert_eq!(stx.priority_op_id.unwrap() as u64, l1_data.serial_id.0);
        assert_eq!(
            stx.gas_limit.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.gas_limit
        );
        assert_eq!(
            stx.max_fee_per_gas.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.max_fee_per_gas
        );
        assert_eq!(
            stx.l1_tx_mint.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.to_mint
        );
        assert_eq!(
            Address::from_slice(stx.l1_tx_refund_recipient.unwrap().as_slice()),
            l1_data.refund_recipient
        );
        assert_eq!(
            stx.gas_per_pubdata_limit
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.gas_per_pubdata_limit
        );
        assert_eq!(stx.l1_block_number.unwrap() as u64, l1_data.eth_block);
        assert_eq!(stx.hash.as_slice(), l1_data.canonical_tx_hash.as_bytes());
    } else {
        panic!("Invalid transaction");
    }
}

#[test]
fn storage_tx_to_l1_tx_with_defaults() {
    let tx_with_defaults = Transaction::from(StorageTransaction {
        l1_tx_refund_recipient: None,
        max_fee_per_gas: None,
        l1_block_number: None,
        ..l1_storage_tx()
    });

    if let ExecuteTransactionCommon::L1(l1_data) = tx_with_defaults.common_data {
        assert_eq!(0, l1_data.eth_block);
        assert_eq!(H160::default(), l1_data.refund_recipient);
        assert_eq!(U256::zero(), l1_data.max_fee_per_gas);
    } else {
        panic!("Invalid transaction");
    }
}

#[test]
fn storage_tx_to_protocol_upgrade_tx() {
    let stx = protocol_upgrade_storage_tx();
    let tx = Transaction::from(stx.clone());

    let execute: Execute = serde_json::from_value(stx.data.clone()).unwrap();
    assert_eq!(execute, tx.execute);
    assert_ne!(0, tx.received_timestamp_ms);
    assert_eq!(None, tx.raw_bytes);

    if let ExecuteTransactionCommon::ProtocolUpgrade(l1_data) = tx.common_data {
        assert_eq!(
            Address::from_slice(stx.initiator_address.as_slice()),
            l1_data.sender
        );
        assert_eq!(stx.upgrade_id.unwrap() as u16, l1_data.upgrade_id as u16);
        assert_eq!(
            stx.gas_limit.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.gas_limit
        );
        assert_eq!(
            stx.max_fee_per_gas.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.max_fee_per_gas
        );
        assert_eq!(
            stx.l1_tx_mint.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.to_mint
        );
        assert_eq!(
            Address::from_slice(stx.l1_tx_refund_recipient.unwrap().as_slice()),
            l1_data.refund_recipient
        );
        assert_eq!(
            stx.gas_per_pubdata_limit
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.gas_per_pubdata_limit
        );
        assert_eq!(stx.l1_block_number.unwrap() as u64, l1_data.eth_block);
        assert_eq!(stx.hash.as_slice(), l1_data.canonical_tx_hash.as_bytes());
    } else {
        panic!("Invalid transaction");
    }
}

#[test]
fn storage_tx_to_protocol_upgrade_tx_with_defaults() {
    let stx = protocol_upgrade_storage_tx();
    let tx_with_defaults = Transaction::from(StorageTransaction {
        l1_tx_mint: None,
        max_fee_per_gas: None,
        l1_block_number: None,
        ..stx.clone()
    });

    if let ExecuteTransactionCommon::ProtocolUpgrade(l1_data) = tx_with_defaults.common_data {
        assert_eq!(U256::default(), l1_data.to_mint);
        assert_eq!(U256::default(), l1_data.max_fee_per_gas);
        assert_eq!(0, l1_data.eth_block);
    } else {
        panic!("Invalid transaction");
    }
}

/// Tests storage transaction to layer 2 transaction logic with different transaction types
fn storage_tx_to_l2_tx(i_tx_format: i32, o_tx_format: i32) {
    let stx = l2_storage_tx(i_tx_format);
    let tx = Transaction::from(stx.clone());

    let execute: Execute = serde_json::from_value(stx.data.clone()).unwrap();
    assert_eq!(execute, tx.execute);
    assert_ne!(0, tx.received_timestamp_ms);
    assert_eq!(stx.input.clone().map(Bytes::from), tx.raw_bytes);

    if let ExecuteTransactionCommon::L2(l1_data) = tx.common_data {
        assert_eq!(stx.nonce.unwrap() as u32, l1_data.nonce.0);
        assert_eq!(
            Fee {
                gas_limit: stx.gas_limit.map(bigdecimal_to_u256).unwrap(),
                max_fee_per_gas: stx.max_fee_per_gas.map(bigdecimal_to_u256).unwrap(),
                max_priority_fee_per_gas: stx
                    .max_priority_fee_per_gas
                    .map(bigdecimal_to_u256)
                    .unwrap(),
                gas_per_pubdata_limit: stx.gas_per_pubdata_limit.map(bigdecimal_to_u256).unwrap()
            },
            l1_data.fee
        );
        assert_eq!(
            stx.initiator_address.as_slice(),
            l1_data.initiator_address.as_bytes()
        );
        assert_eq!(stx.signature.unwrap(), l1_data.signature);
        assert_eq!(o_tx_format, l1_data.transaction_type as i32);
        assert_eq!(stx.input.unwrap(), l1_data.input.clone().unwrap().data);
        assert_eq!(stx.hash.as_slice(), l1_data.hash().as_bytes());
    } else {
        panic!("Invalid transaction");
    }
}

#[test]
fn storage_tx_to_l2_tx_eip712() {
    storage_tx_to_l2_tx(
        EIP_712_TX_TYPE.into(),
        TransactionType::EIP712Transaction as i32,
    );
}

#[test]
fn storage_tx_to_l2_tx_eip2930() {
    storage_tx_to_l2_tx(
        EIP_2930_TX_TYPE.into(),
        TransactionType::EIP2930Transaction as i32,
    );
}

#[test]
fn storage_tx_to_l2_tx_eip1559() {
    storage_tx_to_l2_tx(
        EIP_1559_TX_TYPE.into(),
        TransactionType::EIP1559Transaction as i32,
    );
}

#[test]
fn storage_tx_to_l2_tx_legacy() {
    storage_tx_to_l2_tx(0, TransactionType::LegacyTransaction as i32);
}

#[test]
#[should_panic(expected = "Unsupported tx type")]
fn storage_tx_to_l2_tx_unsupported_tx_type() {
    let stx = l2_storage_tx(1984);
    _ = Transaction::from(stx.clone());
}
