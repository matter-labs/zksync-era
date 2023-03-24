use std::time::Duration;

use db_test_macro::db_test;
use zksync_types::block::{L1BatchHeader, MiniblockHeader};
use zksync_types::proofs::AggregationRound;
use zksync_types::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    fee::{Fee, TransactionExecutionMetrics},
    helpers::unix_timestamp_ms,
    l1::{L1Tx, OpProcessingType, PriorityQueueType},
    l2::L2Tx,
    tx::{tx_execution_info::TxExecutionStatus, TransactionExecutionResult},
    Address, Execute, L1BatchNumber, L1BlockNumber, L1TxCommonData, L2ChainId, MiniblockNumber,
    PriorityOpId, H160, H256, U256,
};

use crate::blocks_dal::BlocksDal;
use crate::prover_dal::{GetProverJobsParams, ProverDal};
use crate::transactions_dal::L2TxSubmissionResult;
use crate::transactions_dal::TransactionsDal;
use crate::transactions_web3_dal::TransactionsWeb3Dal;

fn mock_tx_execution_metrics() -> TransactionExecutionMetrics {
    TransactionExecutionMetrics::default()
}

const DEFAULT_GAS_PER_PUBDATA: u32 = 100;

fn mock_l2_transaction() -> L2Tx {
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
        L2ChainId(270),
        &H256::random(),
        None,
        Default::default(),
    )
    .unwrap();

    l2_tx.set_input(H256::random().0.to_vec(), H256::random());
    l2_tx
}

fn mock_l1_execute() -> L1Tx {
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
        gas_per_pubdata_limit: MAX_GAS_PER_PUBDATA_BYTE.into(),
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

#[db_test(dal_crate)]
async fn workflow_with_submit_tx_equal_hashes(connection_pool: ConnectionPool) {
    let storage = &mut connection_pool.access_test_storage().await;
    let mut transactions_dal = TransactionsDal { storage };

    let tx = mock_l2_transaction();
    let result = transactions_dal.insert_transaction_l2(tx.clone(), mock_tx_execution_metrics());

    assert_eq!(result, L2TxSubmissionResult::Added);

    let result = transactions_dal.insert_transaction_l2(tx, mock_tx_execution_metrics());

    assert_eq!(result, L2TxSubmissionResult::Replaced);
}

#[db_test(dal_crate)]
async fn workflow_with_submit_tx_diff_hashes(connection_pool: ConnectionPool) {
    let storage = &mut connection_pool.access_test_storage().await;
    let mut transactions_dal = TransactionsDal { storage };

    let tx = mock_l2_transaction();

    let nonce = tx.common_data.nonce;
    let initiator_address = tx.common_data.initiator_address;

    let result = transactions_dal.insert_transaction_l2(tx, mock_tx_execution_metrics());

    assert_eq!(result, L2TxSubmissionResult::Added);

    let mut tx = mock_l2_transaction();
    tx.common_data.nonce = nonce;
    tx.common_data.initiator_address = initiator_address;
    let result = transactions_dal.insert_transaction_l2(tx, mock_tx_execution_metrics());

    assert_eq!(result, L2TxSubmissionResult::Replaced);
}

#[db_test(dal_crate)]
async fn remove_stuck_txs(connection_pool: ConnectionPool) {
    let storage = &mut connection_pool.access_test_storage().await;
    let mut transactions_dal = TransactionsDal { storage };
    let storage = &mut connection_pool.access_test_storage().await;
    let mut blocks_dal = BlocksDal { storage };

    // Stuck tx
    let mut tx = mock_l2_transaction();
    tx.received_timestamp_ms = unix_timestamp_ms() - Duration::new(1000, 0).as_millis() as u64;
    transactions_dal.insert_transaction_l2(tx, mock_tx_execution_metrics());
    // Tx in mempool
    let tx = mock_l2_transaction();
    transactions_dal.insert_transaction_l2(tx, mock_tx_execution_metrics());

    // Stuck L1 tx. We should never ever remove L1 tx
    let mut tx = mock_l1_execute();
    tx.received_timestamp_ms = unix_timestamp_ms() - Duration::new(1000, 0).as_millis() as u64;
    transactions_dal.insert_transaction_l1(tx, L1BlockNumber(1));

    // Old executed tx
    let mut executed_tx = mock_l2_transaction();
    executed_tx.received_timestamp_ms =
        unix_timestamp_ms() - Duration::new(1000, 0).as_millis() as u64;
    transactions_dal.insert_transaction_l2(executed_tx.clone(), mock_tx_execution_metrics());

    // Get all txs
    transactions_dal.reset_mempool();
    let txs = transactions_dal.sync_mempool(vec![], vec![], 0, 0, 1000).0;
    assert_eq!(txs.len(), 4);

    blocks_dal.insert_miniblock(MiniblockHeader {
        number: MiniblockNumber(1),
        timestamp: 0,
        hash: Default::default(),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: Default::default(),
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        base_system_contracts_hashes: Default::default(),
    });
    transactions_dal.mark_txs_as_executed_in_miniblock(
        MiniblockNumber(1),
        &[TransactionExecutionResult {
            transaction: executed_tx.clone().into(),
            hash: executed_tx.hash(),
            execution_info: Default::default(),
            execution_status: TxExecutionStatus::Success,
            refunded_gas: 0,
            operator_suggested_refund: 0,
            compressed_bytecodes: vec![],
        }],
        U256::from(1),
    );

    // Get all txs
    transactions_dal.reset_mempool();
    let txs = transactions_dal.sync_mempool(vec![], vec![], 0, 0, 1000).0;
    assert_eq!(txs.len(), 3);

    // Remove one stuck tx
    let removed_txs = transactions_dal.remove_stuck_txs(Duration::from_secs(500));
    assert_eq!(removed_txs, 1);
    transactions_dal.reset_mempool();
    let txs = transactions_dal.sync_mempool(vec![], vec![], 0, 0, 1000).0;
    assert_eq!(txs.len(), 2);

    // We shouldn't collect executed tx
    let storage = &mut connection_pool.access_test_storage().await;
    let mut transactions_web3_dal = TransactionsWeb3Dal { storage };
    transactions_web3_dal
        .get_transaction_receipt(executed_tx.hash())
        .unwrap()
        .unwrap();
}

#[db_test(dal_crate)]
async fn test_duplicate_insert_prover_jobs(connection_pool: ConnectionPool) {
    let storage = &mut connection_pool.access_test_storage().await;
    let block_number = 1;
    let header = L1BatchHeader::new(
        L1BatchNumber(block_number),
        0,
        Default::default(),
        Default::default(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(header, Default::default());

    let mut prover_dal = ProverDal { storage };
    let circuits: Vec<String> = vec![
        "Main VM".to_string(),
        "SHA256".to_string(),
        "Code decommitter".to_string(),
        "Log demuxer".to_string(),
    ];
    let l1_batch_number = L1BatchNumber(block_number);
    prover_dal.insert_prover_jobs(
        l1_batch_number,
        circuits.clone(),
        AggregationRound::BasicCircuits,
    );

    // try inserting the same jobs again to ensure it does not panic
    prover_dal.insert_prover_jobs(
        l1_batch_number,
        circuits.clone(),
        AggregationRound::BasicCircuits,
    );

    let prover_jobs_params = GetProverJobsParams {
        statuses: None,
        blocks: Some(std::ops::Range {
            start: l1_batch_number,
            end: l1_batch_number + 1,
        }),
        limit: None,
        desc: false,
        round: None,
    };
    let jobs = prover_dal.get_jobs(prover_jobs_params).unwrap();
    assert_eq!(circuits.len(), jobs.len());
}
