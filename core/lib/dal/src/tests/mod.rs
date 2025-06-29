use std::time::Duration;

use chrono::DateTime;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_types::{
    block::{L1BatchHeader, L2BlockHasher, L2BlockHeader},
    commitment::PubdataParams,
    fee::Fee,
    fee_model::BatchFeeInput,
    helpers::unix_timestamp_ms,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    l2::L2Tx,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    protocol_upgrade::{ProtocolUpgradeTx, ProtocolUpgradeTxCommonData},
    snapshots::SnapshotRecoveryStatus,
    Address, Execute, K256PrivateKey, L1BatchNumber, L1BlockNumber, L1TxCommonData, L2BlockNumber,
    L2ChainId, PriorityOpId, ProtocolVersion, ProtocolVersionId, H160, H256, U256,
};
use zksync_vm_interface::{
    tracer::ValidationTraces, TransactionExecutionMetrics, TransactionExecutionResult,
    TxExecutionStatus, VmEvent, VmExecutionMetrics,
};

use crate::{
    blocks_dal::BlocksDal,
    protocol_versions_dal::ProtocolVersionsDal,
    transactions_dal::{L2TxSubmissionResult, TransactionsDal},
    transactions_web3_dal::TransactionsWeb3Dal,
    Connection, Core,
};

const DEFAULT_GAS_PER_PUBDATA: u32 = 100;

fn mock_tx_execution_metrics() -> TransactionExecutionMetrics {
    TransactionExecutionMetrics::default()
}

pub(crate) fn create_l2_block_header(number: u32) -> L2BlockHeader {
    let number = L2BlockNumber(number);
    let protocol_version = ProtocolVersionId::default();
    L2BlockHeader {
        number,
        timestamp: number.0.into(),
        hash: L2BlockHasher::new(number, 0, H256::zero()).finalize(protocol_version),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::default(),
        gas_per_pubdata_limit: 100,
        base_fee_per_gas: 100,
        batch_fee_input: BatchFeeInput::l1_pegged(100, 100),
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(protocol_version),
        virtual_blocks: 1,
        gas_limit: 0,
        logs_bloom: Default::default(),
        pubdata_params: PubdataParams::default(),
    }
}

pub(crate) fn create_l1_batch_header(number: u32) -> L1BatchHeader {
    L1BatchHeader::new(
        L1BatchNumber(number),
        100,
        BaseSystemContractsHashes {
            bootloader: H256::repeat_byte(1),
            default_aa: H256::repeat_byte(42),
            evm_emulator: Some(H256::repeat_byte(43)),
        },
        ProtocolVersionId::latest(),
    )
}

pub(crate) fn mock_l2_transaction() -> L2Tx {
    let fee = Fee {
        gas_limit: U256::from(1_000_000u32),
        max_fee_per_gas: U256::from(250_000_000u32),
        max_priority_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::from(DEFAULT_GAS_PER_PUBDATA),
    };
    let mut l2_tx = L2Tx::new_signed(
        Some(Address::random()),
        vec![],
        zksync_types::Nonce(0),
        fee,
        Default::default(),
        L2ChainId::from(270),
        &K256PrivateKey::random(),
        vec![],
        Default::default(),
    )
    .unwrap();

    l2_tx.set_input(H256::random().0.to_vec(), H256::random());
    l2_tx
}

pub(crate) fn mock_l1_execute() -> L1Tx {
    let serial_id = 1;
    let priority_op_data = L1TxCommonData {
        sender: H160::random(),
        canonical_tx_hash: H256::from_low_u64_be(serial_id),
        serial_id: PriorityOpId(serial_id),
        layer_2_tip_fee: U256::zero(),
        full_fee: U256::zero(),
        gas_limit: U256::from(100_100),
        max_fee_per_gas: U256::from(1u32),
        gas_per_pubdata_limit: 100.into(),
        op_processing_type: OpProcessingType::Common,
        priority_queue_type: PriorityQueueType::Deque,
        to_mint: U256::zero(),
        refund_recipient: Address::random(),
        // DEPRECATED.
        eth_block: 0,
    };

    let execute = Execute {
        contract_address: Some(H160::random()),
        value: Default::default(),
        calldata: vec![],
        factory_deps: vec![],
    };

    L1Tx {
        common_data: priority_op_data,
        execute,
        received_timestamp_ms: 0,
    }
}

pub(crate) fn mock_protocol_upgrade_transaction() -> ProtocolUpgradeTx {
    let serial_id = 1;
    let priority_op_data = ProtocolUpgradeTxCommonData {
        sender: H160::random(),
        upgrade_id: Default::default(),
        canonical_tx_hash: H256::from_low_u64_be(serial_id),
        gas_limit: U256::from(100_100),
        max_fee_per_gas: U256::from(1u32),
        gas_per_pubdata_limit: 100.into(),
        to_mint: U256::zero(),
        refund_recipient: Address::random(),
        eth_block: 1,
    };

    let execute = Execute {
        contract_address: Some(H160::random()),
        value: Default::default(),
        calldata: vec![],
        factory_deps: vec![],
    };

    ProtocolUpgradeTx {
        common_data: priority_op_data,
        execute,
        received_timestamp_ms: 0,
    }
}

pub(crate) fn mock_execution_result(transaction: L2Tx) -> TransactionExecutionResult {
    TransactionExecutionResult {
        hash: transaction.hash(),
        transaction: transaction.into(),
        execution_info: VmExecutionMetrics::default(),
        execution_status: TxExecutionStatus::Success,
        refunded_gas: 0,
        call_traces: vec![],
        revert_reason: None,
    }
}

pub(crate) fn create_snapshot_recovery() -> SnapshotRecoveryStatus {
    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(23),
        l1_batch_timestamp: 23,
        l1_batch_root_hash: H256::zero(),
        l2_block_number: L2BlockNumber(42),
        l2_block_timestamp: 42,
        l2_block_hash: H256::zero(),
        protocol_version: ProtocolVersionId::latest(),
        storage_logs_chunks_processed: vec![true; 100],
    }
}

pub(crate) fn mock_vm_event(index: u8) -> VmEvent {
    VmEvent {
        location: (L1BatchNumber(1), u32::from(index)),
        address: Address::repeat_byte(index),
        indexed_topics: (0..4).map(H256::repeat_byte).collect(),
        value: vec![index],
    }
}

pub(crate) fn create_l2_to_l1_log(tx_number_in_block: u16, index: u8) -> UserL2ToL1Log {
    UserL2ToL1Log(L2ToL1Log {
        shard_id: 0,
        is_service: false,
        tx_number_in_block,
        sender: Address::repeat_byte(index),
        key: H256::from_low_u64_be(u64::from(index)),
        value: H256::repeat_byte(index),
    })
}

#[tokio::test]
async fn workflow_with_submit_tx_equal_hashes() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let storage = &mut connection_pool.connection().await.unwrap();
    let mut transactions_dal = TransactionsDal { storage };

    let tx = mock_l2_transaction();
    let result = transactions_dal
        .insert_transaction_l2(
            &tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Added);

    let result = transactions_dal
        .insert_transaction_l2(
            &tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Duplicate);
}

#[tokio::test]
async fn workflow_with_submit_tx_diff_hashes() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let storage = &mut connection_pool.connection().await.unwrap();
    let mut transactions_dal = TransactionsDal { storage };

    let tx = mock_l2_transaction();

    let nonce = tx.common_data.nonce;
    let initiator_address = tx.common_data.initiator_address;

    let result = transactions_dal
        .insert_transaction_l2(
            &tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Added);

    let mut tx = mock_l2_transaction();
    tx.common_data.nonce = nonce;
    tx.common_data.initiator_address = initiator_address;
    let result = transactions_dal
        .insert_transaction_l2(
            &tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Replaced);
}

async fn force_transaction_timestamp(
    storage: &mut Connection<'_, Core>,
    tx_hash: H256,
    timestamp_ms: u64,
) {
    let timestamp_ms = timestamp_ms.try_into().unwrap();
    let result = sqlx::query("UPDATE transactions SET received_at = $2 WHERE hash = $1::bytea")
        .bind(tx_hash.as_bytes())
        .bind(
            DateTime::from_timestamp_millis(timestamp_ms)
                .unwrap()
                .naive_utc(),
        )
        .execute(storage.conn())
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1, "no transaction");
}

#[tokio::test]
async fn remove_stuck_txs() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let storage = &mut connection_pool.connection().await.unwrap();
    let mut protocol_versions_dal = ProtocolVersionsDal { storage };
    protocol_versions_dal
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    let storage = protocol_versions_dal.storage;
    let mut transactions_dal = TransactionsDal { storage };

    // Stuck tx
    let tx = mock_l2_transaction();
    transactions_dal
        .insert_transaction_l2(
            &tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();
    let old_timestamp_ms = unix_timestamp_ms() - 1_000_000;
    force_transaction_timestamp(transactions_dal.storage, tx.hash(), old_timestamp_ms).await;

    // Tx in mempool
    let tx = mock_l2_transaction();
    transactions_dal
        .insert_transaction_l2(
            &tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();

    // Stuck L1 tx. We should never ever remove L1 tx
    let tx = mock_l1_execute();
    transactions_dal
        .insert_transaction_l1(&tx, L1BlockNumber(1))
        .await
        .unwrap();
    force_transaction_timestamp(transactions_dal.storage, tx.hash(), old_timestamp_ms).await;

    // Old executed tx
    let executed_tx = mock_l2_transaction();
    transactions_dal
        .insert_transaction_l2(
            &executed_tx,
            mock_tx_execution_metrics(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();
    force_transaction_timestamp(
        transactions_dal.storage,
        executed_tx.hash(),
        old_timestamp_ms,
    )
    .await;

    // Get all txs
    transactions_dal.reset_mempool().await.unwrap();
    let txs = transactions_dal
        .sync_mempool(&[], &[], 0, 0, true, 1000)
        .await
        .unwrap();
    assert_eq!(txs.len(), 4);

    let storage = transactions_dal.storage;
    BlocksDal { storage }
        .insert_l2_block(&create_l2_block_header(1), L1BatchNumber(1))
        .await
        .unwrap();

    let mut transactions_dal = TransactionsDal { storage };
    transactions_dal
        .mark_txs_as_executed_in_l2_block(
            L2BlockNumber(1),
            &[mock_execution_result(executed_tx.clone())],
            U256::from(1),
            ProtocolVersionId::latest(),
            false,
        )
        .await
        .unwrap();

    // Get all txs
    transactions_dal.reset_mempool().await.unwrap();
    let txs = transactions_dal
        .sync_mempool(&[], &[], 0, 0, true, 1000)
        .await
        .unwrap();
    assert_eq!(txs.len(), 3);

    // Remove one stuck tx
    let removed_txs = transactions_dal
        .remove_stuck_txs(Duration::from_secs(500))
        .await
        .unwrap();
    assert_eq!(removed_txs, 1);
    transactions_dal.reset_mempool().await.unwrap();
    let txs = transactions_dal
        .sync_mempool(&[], &[], 0, 0, true, 1000)
        .await
        .unwrap();
    assert_eq!(txs.len(), 2);

    // We shouldn't collect executed tx
    let storage = transactions_dal.storage;
    let mut transactions_web3_dal = TransactionsWeb3Dal { storage };
    let receipts = transactions_web3_dal
        .get_transaction_receipts(&[executed_tx.hash()])
        .await
        .unwrap();

    assert_eq!(receipts.len(), 1);
}
