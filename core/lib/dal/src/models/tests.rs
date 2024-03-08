use bigdecimal::BigDecimal;
use chrono::Utc;
use zksync_types::{
    l1::{OpProcessingType, PriorityQueueType},
    Address, Execute, ExecuteTransactionCommon, Transaction, H160, H256,
    PRIORITY_OPERATION_L2_TX_TYPE, U256,
};
use zksync_utils::bigdecimal_to_u256;

use crate::models::storage_transaction::StorageTransaction;

#[test]
fn test_storage_transaction_to_l1_transaction_common() {
    let tx_format = PRIORITY_OPERATION_L2_TX_TYPE as i32;
    let execute = Execute {
        contract_address: H160::random(),
        value: U256::zero(),
        calldata: vec![],
        factory_deps: None,
    };
    let hash = H256::random();
    let initiator_address = Address::random();
    let l1_tx_refund_recipient = Address::random();

    let storage_tx = StorageTransaction {
        gas_limit: Some(BigDecimal::from(999)),
        full_fee: Some(BigDecimal::from(888)),
        layer_2_tip_fee: Some(BigDecimal::from(777)),
        l1_tx_mint: Some(BigDecimal::from(666)),
        l1_tx_refund_recipient: Some(l1_tx_refund_recipient.as_bytes().to_vec()),
        hash: hash.as_bytes().to_vec(),
        initiator_address: initiator_address.as_bytes().to_vec(),
        priority_op_id: Some(1),
        max_fee_per_gas: Some(BigDecimal::from(555)),
        gas_per_pubdata_limit: Some(BigDecimal::from(444)),
        l1_block_number: Some(1),
        data: serde_json::to_value(execute.clone()).expect("invalid value"),
        received_at: Utc::now().naive_utc(),
        tx_format: Some(tx_format),
        // Unused fields are intentionally set to their default values
        is_priority: false,
        nonce: None,
        signature: None,
        max_priority_fee_per_gas: None,
        gas_per_storage_limit: None,
        in_mempool: false,
        l1_batch_number: None,
        l1_batch_tx_index: None,
        miniblock_number: None,
        index_in_block: None,
        error: None,
        effective_gas_price: None,
        contract_address: None,
        value: Default::default(),
        paymaster: vec![],
        paymaster_input: vec![],
        refunded_gas: 0,
        execution_info: Default::default(),
        upgrade_id: None,
        created_at: Default::default(),
        input: None,
        updated_at: Default::default(),
    };

    let tx = Transaction::from(storage_tx.clone());

    assert_eq!(execute, tx.execute);
    assert_ne!(0, tx.received_timestamp_ms);
    assert_eq!(None, tx.raw_bytes);

    if let ExecuteTransactionCommon::L1(l1_data) = tx.common_data {
        assert_eq!(
            storage_tx.full_fee.clone().map(bigdecimal_to_u256).unwrap(),
            l1_data.full_fee
        );
        assert_eq!(
            storage_tx
                .layer_2_tip_fee
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.layer_2_tip_fee
        );
        assert_eq!(PriorityQueueType::Deque, l1_data.priority_queue_type);
        assert_eq!(OpProcessingType::Common, l1_data.op_processing_type);
        assert_eq!(initiator_address, l1_data.sender);
        assert_eq!(
            storage_tx.priority_op_id.unwrap() as u64,
            l1_data.serial_id.0
        );
        assert_eq!(
            storage_tx
                .gas_limit
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.gas_limit
        );
        assert_eq!(
            storage_tx
                .max_fee_per_gas
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.max_fee_per_gas
        );
        assert_eq!(
            storage_tx
                .l1_tx_mint
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.to_mint
        );
        assert_eq!(l1_tx_refund_recipient, l1_data.refund_recipient);
        assert_eq!(
            storage_tx
                .gas_per_pubdata_limit
                .clone()
                .map(bigdecimal_to_u256)
                .unwrap(),
            l1_data.gas_per_pubdata_limit
        );
        assert_eq!(0, l1_data.deadline_block);
        assert_eq!(l1_data.eth_hash, Default::default());
        assert_eq!(
            storage_tx.l1_block_number.unwrap() as u64,
            l1_data.eth_block
        );
        assert_eq!(hash, l1_data.canonical_tx_hash);
    } else {
        panic!("Invalid transaction");
    }

    // verify defaults
    let tx_with_defaults = Transaction::from(StorageTransaction {
        l1_tx_refund_recipient: None,
        max_fee_per_gas: None,
        l1_block_number: None,
        ..storage_tx.clone()
    });
    if let ExecuteTransactionCommon::L1(l1_data) = tx_with_defaults.common_data {
        assert_eq!(0, l1_data.eth_block);
        assert_eq!(H160::default(), l1_data.refund_recipient);
        assert_eq!(U256::zero(), l1_data.max_fee_per_gas);
    } else {
        panic!("Invalid transaction");
    }
}
