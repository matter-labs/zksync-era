use std::time::Duration;

use zksync_contracts::BaseSystemContractsHashes;
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_types::{
    block::{MiniblockHasher, MiniblockHeader},
    fee::{Fee, TransactionExecutionMetrics},
    fee_model::BatchFeeInput,
    helpers::unix_timestamp_ms,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    l2::L2Tx,
    protocol_upgrade::{ProtocolUpgradeTx, ProtocolUpgradeTxCommonData},
    snapshots::SnapshotRecoveryStatus,
    tx::{tx_execution_info::TxExecutionStatus, ExecutionMetrics, TransactionExecutionResult},
    Address, Execute, L1BatchNumber, L1BlockNumber, L1TxCommonData, L2ChainId, MiniblockNumber,
    PriorityOpId, ProtocolVersionId, H160, H256, U256,
};

use crate::{
    blocks_dal::BlocksDal,
    protocol_versions_dal::ProtocolVersionsDal,
    transactions_dal::{L2TxSubmissionResult, TransactionsDal},
    transactions_web3_dal::TransactionsWeb3Dal,
    Core,
};

const DEFAULT_GAS_PER_PUBDATA: u32 = 100;

fn mock_tx_execution_metrics() -> TransactionExecutionMetrics {
    TransactionExecutionMetrics::default()
}

pub(crate) fn create_miniblock_header(number: u32) -> MiniblockHeader {
    let number = MiniblockNumber(number);
    let protocol_version = ProtocolVersionId::default();
    MiniblockHeader {
        number,
        timestamp: number.0.into(),
        hash: MiniblockHasher::new(number, 0, H256::zero()).finalize(protocol_version),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::default(),
        gas_per_pubdata_limit: 100,
        base_fee_per_gas: 100,
        batch_fee_input: BatchFeeInput::l1_pegged(100, 100),
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(protocol_version),
        virtual_blocks: 1,
    }
}

pub(crate) fn mock_l2_transaction() -> L2Tx {
    let fee = Fee {
        gas_limit: U256::from(1_000_000u32),
        max_fee_per_gas: U256::from(250_000_000u32),
        max_priority_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::from(DEFAULT_GAS_PER_PUBDATA),
    };
    let mut l2_tx = L2Tx::new_signed(
        Address::random(),
        vec![],
        zksync_types::Nonce(0),
        fee,
        Default::default(),
        L2ChainId::from(270),
        &H256::random(),
        None,
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
        deadline_block: 100000,
        layer_2_tip_fee: U256::zero(),
        full_fee: U256::zero(),
        gas_limit: U256::from(100_100),
        max_fee_per_gas: U256::from(1u32),
        gas_per_pubdata_limit: 100.into(),
        op_processing_type: OpProcessingType::Common,
        priority_queue_type: PriorityQueueType::Deque,
        eth_hash: H256::random(),
        to_mint: U256::zero(),
        refund_recipient: Address::random(),
        eth_block: 1,
    };

    let execute = Execute {
        contract_address: H160::random(),
        value: Default::default(),
        calldata: vec![],
        factory_deps: None,
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
        eth_hash: H256::random(),
        to_mint: U256::zero(),
        refund_recipient: Address::random(),
        eth_block: 1,
    };

    let execute = Execute {
        contract_address: H160::random(),
        value: Default::default(),
        calldata: vec![],
        factory_deps: None,
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
        execution_info: ExecutionMetrics::default(),
        execution_status: TxExecutionStatus::Success,
        refunded_gas: 0,
        operator_suggested_refund: 0,
        compressed_bytecodes: vec![],
        call_traces: vec![],
        revert_reason: None,
    }
}

pub(crate) fn create_snapshot_recovery() -> SnapshotRecoveryStatus {
    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(23),
        l1_batch_timestamp: 23,
        l1_batch_root_hash: H256::zero(),
        miniblock_number: MiniblockNumber(42),
        miniblock_timestamp: 42,
        miniblock_hash: H256::zero(),
        protocol_version: ProtocolVersionId::latest(),
        storage_logs_chunks_processed: vec![true; 100],
    }
}

#[tokio::test]
async fn workflow_with_submit_tx_equal_hashes() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let storage = &mut connection_pool.connection().await.unwrap();
    let mut transactions_dal = TransactionsDal { storage };

    let tx = mock_l2_transaction();
    let result = transactions_dal
        .insert_transaction_l2(tx.clone(), mock_tx_execution_metrics())
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Added);

    let result = transactions_dal
        .insert_transaction_l2(tx, mock_tx_execution_metrics())
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
        .insert_transaction_l2(tx, mock_tx_execution_metrics())
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Added);

    let mut tx = mock_l2_transaction();
    tx.common_data.nonce = nonce;
    tx.common_data.initiator_address = initiator_address;
    let result = transactions_dal
        .insert_transaction_l2(tx, mock_tx_execution_metrics())
        .await
        .unwrap();

    assert_eq!(result, L2TxSubmissionResult::Replaced);
}

#[tokio::test]
async fn remove_stuck_txs() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let storage = &mut connection_pool.connection().await.unwrap();
    let mut protocol_versions_dal = ProtocolVersionsDal { storage };
    protocol_versions_dal
        .save_protocol_version_with_tx(Default::default())
        .await;

    let storage = protocol_versions_dal.storage;
    let mut transactions_dal = TransactionsDal { storage };

    // Stuck tx
    let mut tx = mock_l2_transaction();
    tx.received_timestamp_ms = unix_timestamp_ms() - Duration::new(1000, 0).as_millis() as u64;
    transactions_dal
        .insert_transaction_l2(tx, mock_tx_execution_metrics())
        .await
        .unwrap();
    // Tx in mempool
    let tx = mock_l2_transaction();
    transactions_dal
        .insert_transaction_l2(tx, mock_tx_execution_metrics())
        .await
        .unwrap();

    // Stuck L1 tx. We should never ever remove L1 tx
    let mut tx = mock_l1_execute();
    tx.received_timestamp_ms = unix_timestamp_ms() - Duration::new(1000, 0).as_millis() as u64;
    transactions_dal
        .insert_transaction_l1(tx, L1BlockNumber(1))
        .await;

    // Old executed tx
    let mut executed_tx = mock_l2_transaction();
    executed_tx.received_timestamp_ms =
        unix_timestamp_ms() - Duration::new(1000, 0).as_millis() as u64;
    transactions_dal
        .insert_transaction_l2(executed_tx.clone(), mock_tx_execution_metrics())
        .await
        .unwrap();

    // Get all txs
    transactions_dal.reset_mempool().await.unwrap();
    let txs = transactions_dal
        .sync_mempool(&[], &[], 0, 0, 1000)
        .await
        .unwrap();
    assert_eq!(txs.len(), 4);

    let storage = transactions_dal.storage;
    BlocksDal { storage }
        .insert_miniblock(&create_miniblock_header(1))
        .await
        .unwrap();

    let mut transactions_dal = TransactionsDal { storage };
    transactions_dal
        .mark_txs_as_executed_in_miniblock(
            MiniblockNumber(1),
            &[mock_execution_result(executed_tx.clone())],
            U256::from(1),
        )
        .await;

    // Get all txs
    transactions_dal.reset_mempool().await.unwrap();
    let txs = transactions_dal
        .sync_mempool(&[], &[], 0, 0, 1000)
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
        .sync_mempool(&[], &[], 0, 0, 1000)
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
