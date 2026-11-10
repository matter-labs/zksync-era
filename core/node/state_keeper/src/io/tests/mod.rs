use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use test_casing::test_casing;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_multivm::{
    interface::{
        executor::{BatchExecutor, BatchExecutorFactory},
        storage::{InMemoryStorage, StorageView},
        tracer::ValidationTraces,
        BatchTransactionExecutionResult, ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv,
        L2BlockEnv, SystemEnv, TransactionExecutionMetrics, VmEvent, VmExecutionMetrics,
        VmExecutionResultAndLogs,
    },
    utils::derive_base_fee_and_gas_per_pubdata,
};
use zksync_node_test_utils::prepare_recovery_snapshot;
use zksync_state::OwnedStorage;
use zksync_system_constants::KNOWN_CODES_STORAGE_ADDRESS;
use zksync_types::{
    block::L2BlockHasher,
    bytecode::BytecodeHash,
    commitment::{L1BatchCommitmentMode, PubdataParams},
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    l1::{L1Tx, L1TxCommonData},
    l2::L2Tx,
    protocol_upgrade::ProtocolUpgradeTx,
    protocol_version::ProtocolSemanticVersion,
    settlement::{SettlementLayer, WorkingSettlementLayer},
    AccountTreeId, Address, Execute, ExecuteTransactionCommon, L1BatchNumber, L1BlockNumber,
    L2BlockNumber, L2ChainId, PriorityOpId, ProtocolVersion, ProtocolVersionId, SLChainId,
    StorageKey, Transaction, TransactionTimeRangeConstraint, H256, U256,
};

use self::tester::Tester;
use crate::{
    io::{seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, StateKeeperIO},
    mempool_actor::l2_tx_filter,
    mempool_guard::MempoolGuard,
    seal_criteria::{NoopSealer, UnexecutableReason},
    testonly::{test_batch_executor::MockReadStorageFactory, BASE_SYSTEM_CONTRACTS},
    tests::{create_execution_result, create_transaction, seconds_since_epoch, Query},
    updates::{L2BlockSealCommand, L2BlockUpdates, UpdatesManager},
    MempoolIO, OutputHandler, StateKeeperBuilder, StateKeeperOutputHandler, StateKeeperPersistence,
};

mod tester;

const COMMITMENT_MODES: [L1BatchCommitmentMode; 2] = [
    L1BatchCommitmentMode::Rollup,
    L1BatchCommitmentMode::Validium,
];

/// Ensure that MempoolIO.filter is correctly initialized right after mempool initialization.
#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn test_filter_initialization(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mempool, _) = tester.create_test_mempool_io(connection_pool).await;

    // Upon initialization, the filter should be set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());
}

/// Ensure that MempoolIO.filter is modified correctly if there is a pending batch upon mempool initialization.
#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn test_filter_with_pending_batch(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut tester = Tester::new(commitment_mode);
    tester.genesis(&connection_pool).await;

    // Insert a sealed batch so there will be a `prev_l1_batch_state_root`.
    // These gas values are random and don't matter for filter calculation as there will be a
    // pending batch the filter will be based off of.
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;

    // Inserting a pending L2 block that isn't included in a sealed batch means there is a pending batch.
    // The gas values are randomly chosen but so affect filter values calculation.

    let fee_input = BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
        l1_gas_price: 100,
        fair_l2_gas_price: 1000,
        fair_pubdata_price: 500,
    });
    tester
        .insert_unsealed_batch(&connection_pool, 2, fee_input)
        .await;
    tester.set_timestamp(2);
    tester
        .insert_l2_block(&connection_pool, 2, 10, fee_input)
        .await;

    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool).await;
    // Before the mempool knows there is a pending batch, the filter is still set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());

    mempool.initialize().await.unwrap();
    let (want_base_fee, want_gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let want_filter = L2TxFilter {
        fee_input,
        fee_per_gas: want_base_fee,
        gas_per_pubdata: want_gas_per_pubdata as u32,
        protocol_version: ProtocolVersionId::latest(),
    };
    assert_eq!(mempool.filter(), &want_filter);
}

/// Ensure that `MempoolIO.filter` is modified correctly if there is no pending batch.
#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn test_filter_with_no_pending_batch(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    tester.genesis(&connection_pool).await;

    // Insert a sealed batch so there will be a `prev_l1_batch_state_root`.
    // These gas values are random and don't matter for filter calculation.
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;

    // Create a copy of the tx filter that the mempool will use.
    let want_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();

    // Create a mempool without pending batch and ensure that filter is not initialized just yet.
    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;
    let (io_cursor, _) = mempool.initialize().await.unwrap();
    assert_eq!(mempool.filter(), &L2TxFilter::default());

    // Insert a transaction that matches the expected filter.
    tester.insert_tx(
        &mut guard,
        want_filter.fee_per_gas,
        want_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint::default(),
    );

    // Now, given that there is a transaction matching the expected filter, waiting for the new batch params
    // should succeed and initialize the filter.
    mempool
        .wait_for_new_batch_params(&io_cursor, Duration::from_secs(10))
        .await
        .expect("No batch params in the test mempool");
    assert_eq!(mempool.filter(), &want_filter);
}

/// A priority transaction cannot be rejected, and reaching `reject()` with one used to stop the
/// node. It now pauses L1->L2 processing and keeps the node sequencing L2 traffic.
#[tokio::test]
async fn unexecutable_l1_tx_pauses_l1_to_l2_processing() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(L1BatchCommitmentMode::Rollup);
    tester.genesis(&connection_pool).await;
    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;

    let l1_tx = tester.insert_l1_tx(&mut guard, PriorityOpId(0));
    let l2_tx = tester.insert_tx(
        &mut guard,
        55,
        555,
        TransactionTimeRangeConstraint::default(),
    );
    assert!(!guard.l1_transactions_paused());

    // The priority transaction is served first, as it is prioritized.
    let next = guard.next_transaction(&L2TxFilter::default()).unwrap().0;
    assert_eq!(next.hash(), l1_tx.hash());

    // Rejecting it must not fail; it pauses instead.
    mempool
        .reject(&l1_tx, UnexecutableReason::Halt(Halt::VMPanic))
        .await
        .expect("rejecting an L1 transaction must not stop the state keeper");
    assert!(guard.l1_transactions_paused());

    // L2 traffic keeps flowing while the priority queue stalls.
    let next = guard.next_transaction(&L2TxFilter::default()).unwrap().0;
    assert_eq!(next.hash(), l2_tx.hash());
    assert!(guard.next_transaction(&L2TxFilter::default()).is_none());

    // A further unexecutable priority transaction is also survivable.
    mempool
        .reject(&l1_tx, UnexecutableReason::Halt(Halt::VMPanic))
        .await
        .unwrap();
    assert!(guard.l1_transactions_paused());
}

/// A batch executor that halts every L1 transaction and accepts every L2 one. It records what it was asked to execute.
///
/// With `halt_l1_txs` cleared it accepts everything, which is how the restart test models a node
/// brought back up with the cause dealt with.
#[derive(Debug, Clone)]
struct HaltsL1TxsExecutor {
    executed: Arc<Mutex<Vec<H256>>>,
    halt_l1_txs: bool,
}

impl BatchExecutorFactory<OwnedStorage> for HaltsL1TxsExecutor {
    fn init_batch(
        &mut self,
        _storage: OwnedStorage,
        _l1_batch_env: L1BatchEnv,
        _system_env: SystemEnv,
        _pubdata_params: PubdataParams,
    ) -> Box<dyn BatchExecutor<OwnedStorage>> {
        Box::new(self.clone())
    }
}

#[async_trait::async_trait]
impl BatchExecutor<OwnedStorage> for HaltsL1TxsExecutor {
    async fn execute_tx(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        self.executed.lock().unwrap().push(tx.hash());
        if tx.is_l1() && self.halt_l1_txs {
            return Ok(BatchTransactionExecutionResult {
                tx_result: Box::new(VmExecutionResultAndLogs::mock(ExecutionResult::Halt {
                    reason: Halt::VMPanic,
                })),
                compression_result: Ok(()),
                call_traces: vec![],
            });
        }
        Ok(crate::testonly::successful_exec())
    }

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn start_next_l2_block(&mut self, _env: L2BlockEnv) -> anyhow::Result<()> {
        Ok(())
    }

    async fn finish_batch(
        self: Box<Self>,
    ) -> anyhow::Result<(FinishedL1Batch, StorageView<OwnedStorage>)> {
        let storage = OwnedStorage::boxed(InMemoryStorage::default());
        Ok((FinishedL1Batch::mock(), StorageView::new(storage)))
    }

    async fn rollback_l2_block(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn commit_l2_block(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// End to end through the real state keeper loop: the real `MempoolIO`, the real keeper, and an L1
/// transaction that halts. The keeper must not die, must pause L1->L2 processing, and
/// must go on to execute the L2 transaction behind it.
///
/// Before the pause existed, `MempoolIO::reject()` returned an error here and the keeper stopped
/// with it — the whole sequencer down on one bad deposit.
#[tokio::test]
async fn state_keeper_survives_an_unexecutable_l1_tx() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(2).await;
    let mut tester = Tester::new(L1BatchCommitmentMode::Rollup);
    tester.genesis(&connection_pool).await;

    // A sealed batch, so the new batch the keeper opens has a previous state root.
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;
    tester.set_timestamp(2);

    let (io, mut guard) = tester.create_test_mempool_io(connection_pool.clone()).await;

    // The L2 transaction has to clear the fee filter the keeper will build, or it would be ignored
    // for reasons that have nothing to do with this test.
    let filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();
    let l1_tx = tester.insert_l1_tx(&mut guard, PriorityOpId(0));
    let l2_tx = tester.insert_tx(
        &mut guard,
        filter.fee_per_gas,
        filter.gas_per_pubdata,
        TransactionTimeRangeConstraint::default(),
    );

    let executed = Arc::new(Mutex::new(Vec::new()));
    let (persistence, l2_block_sealer) =
        StateKeeperPersistence::new(connection_pool.clone(), Some(Address::default()), 5)
            .await
            .unwrap();
    tokio::spawn(l2_block_sealer.run());

    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let state_keeper = StateKeeperBuilder::new(
        Box::new(io),
        Box::new(HaltsL1TxsExecutor {
            executed: executed.clone(),
            halt_l1_txs: true,
        }),
        OutputHandler::new(Box::new(persistence.with_tx_insertion())),
        Arc::new(NoopSealer),
        Arc::new(MockReadStorageFactory),
        None,
    )
    .build(&stop_receiver)
    .await
    .unwrap();
    let keeper_handle = tokio::spawn(state_keeper.run(stop_receiver));

    // Wait for the keeper to have tried the L1 transaction and moved on to the L2 one.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut keeper_died = false;
    let mut timed_out = false;
    loop {
        // The keeper has no reason to finish on its own here: it was not asked to stop. Finishing
        // means it failed, which is exactly the crash this test is about.
        if keeper_handle.is_finished() {
            keeper_died = true;
            break;
        }
        if executed.lock().unwrap().contains(&l2_tx.hash()) {
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if keeper_died {
        let outcome = keeper_handle.await.expect("the state keeper panicked");
        panic!("the state keeper stopped instead of pausing L1->L2 processing: {outcome:?}");
    }
    assert!(
        !timed_out,
        "the L2 transaction was never executed; executed so far: {:?}",
        executed.lock().unwrap()
    );

    // The L1 transaction was tried, halted, and the queue is now paused.
    assert!(
        executed.lock().unwrap().contains(&l1_tx.hash()),
        "the L1 transaction was never executed: {:?}",
        executed.lock().unwrap()
    );
    assert!(
        guard.l1_transactions_paused(),
        "L1->L2 processing should be paused"
    );

    // And no further L1 transaction is served, even though one is buffered and it is the next
    // priority id. Read the live record, not a snapshot taken before it was inserted.
    let l1_tx_2 = tester.insert_l1_tx(&mut guard, PriorityOpId(1));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !executed.lock().unwrap().contains(&l1_tx_2.hash()),
        "a priority transaction was served while paused: {:?}",
        executed.lock().unwrap()
    );

    // The keeper is alive and shuts down cleanly, rather than having died on the rejection.
    assert!(
        !keeper_handle.is_finished(),
        "the state keeper stopped while paused"
    );
    stop_sender.send_replace(true);
    keeper_handle
        .await
        .expect("the state keeper panicked")
        .expect("the state keeper returned an error");
}

/// The pause must not swallow ordinary L2 rejections, which still mark the transaction rejected.
#[tokio::test]
async fn rejecting_an_l2_tx_is_unaffected_by_the_l1_pause() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(L1BatchCommitmentMode::Rollup);
    tester.genesis(&connection_pool).await;
    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;

    let l2_tx = tester.insert_tx(
        &mut guard,
        55,
        555,
        TransactionTimeRangeConstraint::default(),
    );
    guard.next_transaction(&L2TxFilter::default()).unwrap();

    // Still goes down the ordinary rejection path, which marks the transaction rejected in Postgres.
    mempool
        .reject(&l2_tx.into(), UnexecutableReason::Halt(Halt::VMPanic))
        .await
        .unwrap();

    // L1->L2 processing is untouched by an L2 rejection.
    assert!(!guard.l1_transactions_paused());
}

async fn assert_new_batch_settlement_layer(
    commitment_mode: L1BatchCommitmentMode,
    working_settlement_layer: WorkingSettlementLayer,
    expected_settlement_layer: SettlementLayer,
) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    tester.genesis(&connection_pool).await;

    // Prepare a sealed batch so opening a new one follows the regular execution path.
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;

    let (mut mempool, mut guard) = tester
        .create_test_mempool_io_with_settlement_mode(
            connection_pool.clone(),
            working_settlement_layer,
        )
        .await;
    let (io_cursor, _) = mempool.initialize().await.unwrap();

    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();
    tester.insert_tx(
        &mut guard,
        tx_filter.fee_per_gas,
        tx_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint::default(),
    );

    let batch_params = mempool
        .wait_for_new_batch_params(&io_cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("No batch params in the test mempool");
    assert_eq!(batch_params.settlement_layer, expected_settlement_layer);

    let mut storage = connection_pool.connection().await.unwrap();
    let unsealed_batch = storage
        .blocks_dal()
        .get_unsealed_l1_batch()
        .await
        .unwrap()
        .expect("Expected unsealed batch to be inserted");
    assert_eq!(unsealed_batch.settlement_layer, expected_settlement_layer);
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn new_batch_uses_current_settlement_layer_when_migration_is_not_in_progress(
    commitment_mode: L1BatchCommitmentMode,
) {
    let mut settlement_layer = WorkingSettlementLayer::new(
        SettlementLayer::L1(SLChainId(1)),
        SettlementLayer::Gateway(SLChainId(270)),
    );
    settlement_layer.set_migration_in_progress(false);

    assert_new_batch_settlement_layer(
        commitment_mode,
        settlement_layer,
        SettlementLayer::L1(SLChainId(1)),
    )
    .await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn new_batch_uses_target_settlement_layer_when_migration_is_in_progress(
    commitment_mode: L1BatchCommitmentMode,
) {
    let mut settlement_layer = WorkingSettlementLayer::new(
        SettlementLayer::L1(SLChainId(1)),
        SettlementLayer::Gateway(SLChainId(270)),
    );
    settlement_layer.set_migration_in_progress(true);

    assert_new_batch_settlement_layer(
        commitment_mode,
        settlement_layer,
        SettlementLayer::Gateway(SLChainId(270)),
    )
    .await;
}

async fn test_timestamps(
    connection_pool: ConnectionPool<Core>,
    prev_l1_batch_timestamp: u64,
    prev_l2_block_timestamp: u64,
    mut tester: Tester,
) {
    tester.genesis(&connection_pool).await;

    tester.set_timestamp(prev_l2_block_timestamp);
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester.set_timestamp(prev_l1_batch_timestamp);
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;

    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;
    let (io_cursor, _) = mempool.initialize().await.unwrap();
    // Insert a transaction to trigger L1 batch creation.
    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();
    tester.insert_tx(
        &mut guard,
        tx_filter.fee_per_gas,
        tx_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint::default(),
    );

    let l1_batch_params = mempool
        .wait_for_new_batch_params(&io_cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("No batch params in the test mempool");

    if l1_batch_params
        .protocol_version
        .is_pre_interop_fast_blocks()
    {
        assert!(l1_batch_params.first_l2_block.timestamp() > prev_l2_block_timestamp);
    } else {
        assert!(l1_batch_params.first_l2_block.timestamp() >= prev_l2_block_timestamp);
    }
    assert!(l1_batch_params.first_l2_block.timestamp() > prev_l1_batch_timestamp);
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_basics(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps(
        connection_pool,
        current_timestamp,
        current_timestamp,
        tester,
    )
    .await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_with_clock_skew(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps(
        connection_pool,
        current_timestamp + 2,
        current_timestamp + 2,
        tester,
    )
    .await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_respects_prev_l2_block(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps(
        connection_pool,
        current_timestamp,
        current_timestamp - 1,
        tester,
    )
    .await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_respects_prev_l2_block_with_clock_skew(
    commitment_mode: L1BatchCommitmentMode,
) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps(
        connection_pool,
        current_timestamp + 2,
        current_timestamp + 1,
        tester,
    )
    .await;
}

fn create_block_seal_command(
    l1_batch_number: L1BatchNumber,
    l2_block: L2BlockUpdates,
) -> L2BlockSealCommand {
    L2BlockSealCommand {
        l1_batch_number,
        l2_block,
        first_tx_index: 0,
        fee_account_address: Address::repeat_byte(0x23),
        fee_input: BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
            l1_gas_price: 100,
            fair_l2_gas_price: 100,
            fair_pubdata_price: 100,
        }),
        base_fee_per_gas: 10,
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        l2_legacy_shared_bridge_addr: Some(Address::default()),
        pre_insert_data: false,
        pubdata_params: PubdataParams::genesis(),
        insert_header: true,
        rolling_txs_hash: Default::default(),
        settlement_layer: SettlementLayer::for_tests(),
        call_traces_max_chunk_size: zksync_dal::DEFAULT_CALL_TRACES_MAX_CHUNK_SIZE,
    }
}

#[tokio::test]
async fn processing_storage_logs_when_sealing_l2_block() {
    let connection_pool =
        ConnectionPool::<Core>::constrained_test_pool(L2BlockSealProcess::subtasks_len()).await;
    let mut l2_block = L2BlockUpdates::new(
        0,
        L2BlockNumber(3),
        H256::zero(),
        1,
        ProtocolVersionId::latest(),
        vec![],
    );

    let tx = create_transaction(10, 100);
    let storage_logs = [
        (U256::from(1), Query::Read(U256::from(0))),
        (U256::from(2), Query::InitialWrite(U256::from(1))),
        (
            U256::from(3),
            Query::RepeatedWrite(U256::from(2), U256::from(3)),
        ),
        (
            U256::from(2),
            Query::RepeatedWrite(U256::from(1), U256::from(4)),
        ),
    ];
    let execution_result = create_execution_result(storage_logs);
    l2_block.extend_from_executed_transaction(
        tx,
        execution_result,
        VmExecutionMetrics::default(),
        vec![],
    );

    let tx = create_transaction(10, 100);
    let storage_logs = [
        (U256::from(4), Query::InitialWrite(U256::from(5))),
        (
            U256::from(3),
            Query::RepeatedWrite(U256::from(3), U256::from(6)),
        ),
    ];
    let execution_result = create_execution_result(storage_logs);
    l2_block.extend_from_executed_transaction(
        tx,
        execution_result,
        VmExecutionMetrics::default(),
        vec![],
    );

    let l1_batch_number = L1BatchNumber(2);
    let seal_command = create_block_seal_command(l1_batch_number, l2_block.clone());
    connection_pool
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    seal_command.seal(connection_pool.clone()).await.unwrap();
    let mut conn = connection_pool.connection().await.unwrap();

    // Manually mark the L2 block as executed so that getting touched slots from it works
    conn.blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
        .await
        .unwrap();
    let touched_slots = conn
        .storage_logs_dal()
        .get_touched_slots_for_l1_batch(l1_batch_number)
        .await
        .unwrap();

    // Keys that are only read must not be written to `storage_logs`.
    let account = AccountTreeId::default();
    let read_key = StorageKey::new(account, H256::from_low_u64_be(1));
    assert!(!touched_slots.contains_key(&read_key.hashed_key()));

    // The storage logs must be inserted and read in the correct order, so that
    // `touched_slots` contain the most recent values in the L1 batch.
    assert_eq!(touched_slots.len(), 3);
    let written_kvs = [(2, 4), (3, 6), (4, 5)];
    for (key, value) in written_kvs {
        let key = StorageKey::new(account, H256::from_low_u64_be(key));
        let expected_value = H256::from_low_u64_be(value);
        assert_eq!(touched_slots[&key.hashed_key()], expected_value);
    }
}

#[tokio::test]
async fn processing_events_when_sealing_l2_block() {
    let pool =
        ConnectionPool::<Core>::constrained_test_pool(L2BlockSealProcess::subtasks_len()).await;
    let l1_batch_number = L1BatchNumber(2);
    let l2_block_number = L2BlockNumber(3);
    let mut l2_block = L2BlockUpdates::new(
        0,
        l2_block_number,
        H256::zero(),
        1,
        ProtocolVersionId::latest(),
        vec![],
    );

    let events = (0_u8..10).map(|i| VmEvent {
        location: (l1_batch_number, u32::from(i / 4)),
        value: vec![i],
        ..VmEvent::default()
    });
    let events: Vec<_> = events.collect();

    for events_chunk in events.chunks(4) {
        let tx = create_transaction(10, 100);
        let mut execution_result = create_execution_result([]);
        execution_result.logs.events = events_chunk.to_vec();
        l2_block.extend_from_executed_transaction(
            tx,
            execution_result,
            VmExecutionMetrics::default(),
            vec![],
        );
    }

    let seal_command = create_block_seal_command(l1_batch_number, l2_block.clone());
    pool.connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    seal_command.seal(pool.clone()).await.unwrap();
    let mut conn = pool.connection().await.unwrap();

    let logs = conn
        .events_web3_dal()
        .get_all_logs(l2_block_number - 1)
        .await
        .unwrap();

    assert_eq!(logs.len(), 10);
    // The event logs should be inserted in the correct order.
    for (i, log) in logs.iter().enumerate() {
        assert_eq!(log.data.0, [i as u8]);
    }
}

fn bytecode_publishing_events(
    l1_batch_number: L1BatchNumber,
    tx_index: u32,
    bytecode_hashes: impl Iterator<Item = H256>,
) -> Vec<VmEvent> {
    bytecode_hashes
        .map(|bytecode_hash| VmEvent {
            location: (l1_batch_number, tx_index),
            address: KNOWN_CODES_STORAGE_ADDRESS,
            indexed_topics: vec![
                VmEvent::PUBLISHED_BYTECODE_SIGNATURE,
                bytecode_hash,
                H256::from_low_u64_be(1), // sentBytecodeToL1
            ],
            value: vec![],
        })
        .collect()
}

#[tokio::test]
async fn processing_dynamic_factory_deps_when_sealing_l2_block() {
    let pool =
        ConnectionPool::<Core>::constrained_test_pool(L2BlockSealProcess::subtasks_len()).await;
    let l1_batch_number = L1BatchNumber(2);
    let l2_block_number = L2BlockNumber(3);
    let mut l2_block = L2BlockUpdates::new(
        0,
        l2_block_number,
        H256::zero(),
        1,
        ProtocolVersionId::latest(),
        vec![],
    );

    let static_factory_deps: Vec<_> = (0_u8..10)
        .map(|byte| {
            let era_bytecode = vec![byte; 32];
            (
                BytecodeHash::for_bytecode(&era_bytecode).value(),
                era_bytecode,
            )
        })
        .collect();
    let dynamic_factory_deps: Vec<_> = (0_u8..10)
        .map(|byte| {
            let evm_bytecode = vec![byte; 96];
            (
                BytecodeHash::for_raw_evm_bytecode(&evm_bytecode).value(),
                evm_bytecode,
            )
        })
        .collect();
    let mut all_factory_deps = static_factory_deps.clone();
    all_factory_deps.extend_from_slice(&dynamic_factory_deps);

    let events = bytecode_publishing_events(
        l1_batch_number,
        0,
        static_factory_deps
            .iter()
            .chain(&dynamic_factory_deps)
            .map(|(hash, _)| *hash),
    );

    let mut tx = create_transaction(10, 100);
    tx.execute.factory_deps = static_factory_deps
        .into_iter()
        .map(|(_, bytecode)| bytecode)
        .collect();
    let mut execution_result = create_execution_result([]);
    execution_result.dynamic_factory_deps = dynamic_factory_deps.into_iter().collect();
    execution_result.logs.events = events;
    l2_block.extend_from_executed_transaction(
        tx,
        execution_result,
        VmExecutionMetrics::default(),
        vec![],
    );

    assert_eq!(
        l2_block.new_factory_deps.len(),
        all_factory_deps.len(),
        "{:?}",
        l2_block.new_factory_deps
    );
    for (hash, bytecode) in &all_factory_deps {
        assert_eq!(
            l2_block.new_factory_deps.get(hash),
            Some(bytecode),
            "{hash:?}"
        );
    }

    let seal_command = create_block_seal_command(l1_batch_number, l2_block);
    pool.connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    seal_command.seal(pool.clone()).await.unwrap();

    let mut conn = pool.connection().await.unwrap();
    let persisted_factory_deps = conn
        .factory_deps_dal()
        .dump_all_factory_deps_for_tests()
        .await;
    for (hash, bytecode) in &all_factory_deps {
        assert_eq!(persisted_factory_deps.get(hash), Some(bytecode), "{hash:?}");
    }
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l2_block_processing_after_snapshot_recovery(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let tester = Tester::new(commitment_mode);
    let mut storage = connection_pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let (mut mempool, mut mempool_guard) =
        tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, maybe_pending_batch) = mempool.initialize().await.unwrap();
    assert_eq!(cursor.next_l2_block, snapshot_recovery.l2_block_number + 1);
    assert_eq!(cursor.l1_batch, snapshot_recovery.l1_batch_number + 1);
    assert!(maybe_pending_batch.is_none());

    // Insert a transaction into the mempool in order to open a new batch.
    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();
    let tx = tester.insert_tx(
        &mut mempool_guard,
        tx_filter.fee_per_gas,
        tx_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint::default(),
    );
    insert_l2_transaction(&mut storage, &tx).await;

    let previous_batch_hash = mempool
        .load_batch_state_hash(snapshot_recovery.l1_batch_number)
        .await
        .unwrap();
    assert_eq!(previous_batch_hash, snapshot_recovery.l1_batch_root_hash);

    let l1_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no batch params generated");
    let batch_init_params = l1_batch_params.into_init_params(
        L2ChainId::default(),
        BASE_SYSTEM_CONTRACTS.clone(),
        &cursor,
        previous_batch_hash,
    );
    let version = batch_init_params.system_env.version;
    let mut updates = UpdatesManager::new(
        &batch_init_params,
        version,
        cursor.prev_l1_batch_timestamp,
        Some(cursor.prev_l2_block_timestamp),
        true,
    );

    let tx_hash = tx.hash();
    updates.extend_from_executed_transaction(
        tx.into(),
        create_execution_result([]),
        VmExecutionMetrics::default(),
        vec![],
    );

    let (mut persistence, l2_block_sealer) =
        StateKeeperPersistence::new(connection_pool.clone(), Some(Address::default()), 0)
            .await
            .unwrap();
    tokio::spawn(l2_block_sealer.run());
    persistence.handle_l2_block_data(&updates).await.unwrap();

    // Check that the L2 block is persisted and has correct data.
    let persisted_l2_block = storage
        .blocks_dal()
        .get_l2_block_header(snapshot_recovery.l2_block_number + 1)
        .await
        .unwrap()
        .expect("no L2 block persisted");
    assert_eq!(
        persisted_l2_block.number,
        snapshot_recovery.l2_block_number + 1
    );
    assert_eq!(persisted_l2_block.l2_tx_count, 1);

    let mut l2_block_hasher = L2BlockHasher::new(
        persisted_l2_block.number,
        persisted_l2_block.timestamp,
        snapshot_recovery.l2_block_hash,
    );
    l2_block_hasher.push_tx_hash(tx_hash);
    assert_eq!(
        persisted_l2_block.hash,
        l2_block_hasher.finalize(ProtocolVersionId::latest())
    );

    let l2_block_transactions = storage
        .transactions_web3_dal()
        .get_raw_l2_block_transactions(persisted_l2_block.number)
        .await
        .unwrap();
    assert_eq!(l2_block_transactions.len(), 1);
    assert_eq!(l2_block_transactions[0].hash(), tx_hash);

    // Emulate node restart.
    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, maybe_pending_batch) = mempool.initialize().await.unwrap();
    assert_eq!(cursor.next_l2_block, snapshot_recovery.l2_block_number + 2);
    assert_eq!(cursor.l1_batch, snapshot_recovery.l1_batch_number + 1);

    let pending_batch = maybe_pending_batch.expect("no pending batch");
    assert_eq!(
        pending_batch.l1_batch_env.number,
        snapshot_recovery.l1_batch_number + 1
    );
    assert_eq!(
        pending_batch.l1_batch_env.previous_batch_hash,
        Some(snapshot_recovery.l1_batch_root_hash)
    );
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
        snapshot_recovery.l2_block_hash
    );
    assert_eq!(pending_batch.pending_l2_blocks.len(), 1);
    assert_eq!(
        pending_batch.pending_l2_blocks[0].number,
        snapshot_recovery.l2_block_number + 1
    );
    assert_eq!(
        pending_batch.pending_l2_blocks[0].prev_block_hash,
        snapshot_recovery.l2_block_hash
    );
    assert_eq!(pending_batch.pending_l2_blocks[0].txs.len(), 1);
    assert_eq!(pending_batch.pending_l2_blocks[0].txs[0].hash(), tx_hash);
}

/// Ensure that subsequent L2 blocks that belong to the same L1 batch have different timestamps for <= v28.
#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn different_timestamp_for_l2_blocks_in_same_batch(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);

    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool).await;
    let (mut io_cursor, _) = mempool.initialize().await.unwrap();
    let current_timestamp = seconds_since_epoch();
    io_cursor.prev_l2_block_timestamp = current_timestamp;

    mempool.set_last_batch_protocol_version(ProtocolVersionId::Version28);
    let l2_block_params = mempool
        .wait_for_new_l2_block_params(&io_cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no new L2 block params");
    assert!(l2_block_params.timestamp() > current_timestamp);
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn continue_unsealed_batch_on_restart(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let tester = Tester::new(commitment_mode);
    tester.genesis(&connection_pool).await;
    let mut storage = connection_pool.connection().await.unwrap();

    let (mut mempool, mut mempool_guard) =
        tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, _) = mempool.initialize().await.unwrap();

    // Insert a transaction into the mempool in order to open a new batch.
    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();
    let tx = tester.insert_tx(
        &mut mempool_guard,
        tx_filter.fee_per_gas,
        tx_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint::default(),
    );
    insert_l2_transaction(&mut storage, &tx).await;

    let old_l1_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no batch params generated");

    // Restart
    drop((mempool, mempool_guard, cursor));
    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, _) = mempool.initialize().await.unwrap();

    let mut new_l1_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no batch params generated");

    // Timestamp in millis can be different. So we check only the seconds part.
    assert_eq!(
        old_l1_batch_params.first_l2_block.timestamp(),
        new_l1_batch_params.first_l2_block.timestamp()
    );
    *new_l1_batch_params.first_l2_block.timestamp_ms_mut() =
        old_l1_batch_params.first_l2_block.timestamp_ms();
    assert_eq!(new_l1_batch_params, old_l1_batch_params);
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn insert_unsealed_batch_on_init(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = Tester::new(commitment_mode);
    tester.genesis(&connection_pool).await;
    let fee_input = BatchFeeInput::pubdata_independent(55, 555, 5555);
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, fee_input)
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;
    // Pre-insert L2 block without its unsealed L1 batch counterpart
    tester
        .insert_unsealed_batch(&connection_pool, 2, fee_input)
        .await;
    tester.set_timestamp(2);
    tester
        .insert_l2_block(&connection_pool, 2, 5, fee_input)
        .await;

    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool.clone()).await;
    // Initialization is supposed to recognize that the current L1 batch is not present in the DB and
    // insert it itself.
    let (cursor, _) = mempool.initialize().await.unwrap();

    // Make sure we are able to fetch the newly inserted batch's params
    let l1_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no batch params generated");
    assert_eq!(l1_batch_params.fee_input, fee_input);
    assert_eq!(l1_batch_params.first_l2_block.timestamp(), 2);
}

#[tokio::test]
async fn test_mempool_with_timestamp_assertion() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(2).await;
    // what commitment mode to use is irrelevant here
    let tester = Tester::new(L1BatchCommitmentMode::Rollup);
    let mut storage = connection_pool.connection().await.unwrap();

    tester.genesis(&connection_pool).await;

    // Insert a sealed batch so there will be a `prev_l1_batch_state_root`.
    // These gas values are random and don't matter for filter calculation.
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;

    // Create a copy of the tx filter that the mempool will use.
    let want_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();

    // Create a mempool without pending batch and ensure that filter is not initialized just yet.
    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;
    mempool.initialize().await.unwrap();
    assert_eq!(mempool.filter(), &L2TxFilter::default());

    let system_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    // inserting 3 transactions - a good one, sandwiched in between two bad ones. The good one should
    // be returned by wait_for_next_tx, while two bad ones should be rejected.
    let rejected_tx_1 = tester.insert_tx(
        &mut guard,
        want_filter.fee_per_gas,
        want_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint {
            timestamp_asserter_range: Some(system_time - 20000..system_time - 10000),
        },
    );
    let expected_tx = tester.insert_tx(
        &mut guard,
        want_filter.fee_per_gas,
        want_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint {
            timestamp_asserter_range: Some(system_time - 1000..system_time + 1000),
        },
    );
    let rejected_tx_2 = tester.insert_tx(
        &mut guard,
        want_filter.fee_per_gas,
        want_filter.gas_per_pubdata,
        TransactionTimeRangeConstraint {
            timestamp_asserter_range: Some(system_time + 10000..system_time + 20000),
        },
    );
    insert_l2_transaction(&mut storage, &rejected_tx_1).await;
    insert_l2_transaction(&mut storage, &expected_tx).await;
    insert_l2_transaction(&mut storage, &rejected_tx_2).await;

    let tx = mempool
        .wait_for_next_tx(Duration::from_secs(2), system_time)
        .await
        .unwrap()
        .expect("No expected transaction in the mempool");
    assert_eq!(expected_tx.hash(), tx.hash());

    let next_tx = mempool
        .wait_for_next_tx(Duration::from_secs(2), system_time)
        .await
        .expect("Should be no more transactions in the mempool");
    assert!(next_tx.is_none());

    // verify that two transactions have been rejected
    let rejected_storage_tx_1 = storage
        .transactions_dal()
        .get_storage_tx_by_hash(rejected_tx_1.hash())
        .await
        .unwrap()
        .expect("Failed to find transaction");
    assert_eq!(
        "rejected: Transaction failed block.timestamp assertion",
        rejected_storage_tx_1.error.unwrap()
    );

    let rejected_storage_tx_2 = storage
        .transactions_dal()
        .get_storage_tx_by_hash(rejected_tx_2.hash())
        .await
        .unwrap()
        .expect("Failed to find transaction");
    assert_eq!(
        "rejected: Transaction failed block.timestamp assertion",
        rejected_storage_tx_2.error.unwrap()
    );
}

#[tokio::test]
async fn test_batch_params_with_protocol_upgrade_tx() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(2).await;
    let tester = Tester::new(L1BatchCommitmentMode::Rollup);
    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;

    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, _) = mempool.initialize().await.unwrap();

    // Check that new batch params are not returned when there is no tx to process.
    let new_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_millis(100))
        .await
        .unwrap();
    assert!(new_batch_params.is_none());

    // Insert protocol version with upgrade tx.
    let protocol_upgrade_tx = ProtocolUpgradeTx {
        execute: Default::default(),
        common_data: Default::default(),
        received_timestamp_ms: 0,
    };
    let version = ProtocolVersion {
        version: ProtocolSemanticVersion {
            minor: ProtocolVersionId::next(),
            patch: 0.into(),
        },
        tx: Some(protocol_upgrade_tx),
        ..Default::default()
    };
    connection_pool
        .connection()
        .await
        .unwrap()
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&version)
        .await
        .unwrap();
    let new_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_millis(100))
        .await
        .unwrap();
    assert!(new_batch_params.is_some());
}

async fn insert_l2_transaction(storage: &mut Connection<'_, Core>, tx: &L2Tx) {
    storage
        .transactions_dal()
        .insert_transaction_l2(
            tx,
            TransactionExecutionMetrics::default(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();
}

/// The restart contract, against Postgres: a priority transaction that paused the queue is still
/// there after a restart, and executes.
///
/// This is the only route by which a paused deposit is ever processed, and it is the one claim in the
/// pause design that the other tests cannot reach. They seed the in-memory mempool directly, and a
/// restart throws that mempool away — Postgres is the only thing that carries the deposit across. So
/// this test puts the deposit in the database, pauses on it, and then rebuilds from the database the
/// way a restarted node does.
///
/// One limit worth stating: the `MempoolFetcher` actor is not spun up. The test calls the same two
/// DAL functions its `run()` does, in the same order — `reset_mempool()` then `sync_mempool()` — so
/// what is covered is the database contract between the two runs, not the actor's own loop.
#[tokio::test]
async fn a_paused_l1_tx_is_executed_after_a_restart() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(4).await;
    let mut tester = Tester::new(L1BatchCommitmentMode::Rollup);
    tester.genesis(&connection_pool).await;
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result.hash])
        .await;
    tester.set_timestamp(2);

    // The deposit lives in Postgres, the way a real one does, rather than only in the mempool.
    let l1_tx = L1Tx {
        execute: Execute {
            contract_address: Some(Address::random()),
            calldata: vec![],
            value: U256::zero(),
            factory_deps: vec![],
        },
        common_data: L1TxCommonData {
            sender: Address::random(),
            serial_id: PriorityOpId(0),
            gas_limit: U256::from(100_000),
            gas_per_pubdata_limit: U256::from(800),
            canonical_tx_hash: H256::from_low_u64_be(0xdeadbeef),
            ..L1TxCommonData::default()
        },
        received_timestamp_ms: 0,
    };
    let l1_hash = Transaction::from(l1_tx.clone()).hash();
    connection_pool
        .connection()
        .await
        .unwrap()
        .transactions_dal()
        .insert_transaction_l1(&l1_tx, L1BlockNumber(1))
        .await
        .unwrap();

    let filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest(),
    )
    .await
    .unwrap();

    // Loads pending transactions exactly as `MempoolFetcher` does, priority ones included.
    async fn load_from_postgres(
        pool: &ConnectionPool<Core>,
        filter: &L2TxFilter,
    ) -> Vec<(Transaction, TransactionTimeRangeConstraint)> {
        pool.connection()
            .await
            .unwrap()
            .transactions_dal()
            .sync_mempool(
                &[],
                &[],
                filter.gas_per_pubdata,
                filter.fee_per_gas,
                true,
                10,
            )
            .await
            .unwrap()
    }

    // ---- First run: the executor halts the deposit, so the queue pauses on it. ----
    let executed_before = Arc::new(Mutex::new(Vec::new()));
    {
        let (io, guard) = tester.create_test_mempool_io(connection_pool.clone()).await;
        let loaded = load_from_postgres(&connection_pool, &filter).await;
        assert!(
            loaded.iter().any(|(tx, _)| tx.hash() == l1_hash),
            "the deposit was not loaded from Postgres: {loaded:?}"
        );
        guard.insert(loaded, HashMap::new());

        let (persistence, l2_block_sealer) =
            StateKeeperPersistence::new(connection_pool.clone(), Some(Address::default()), 5)
                .await
                .unwrap();
        let sealer_handle = tokio::spawn(l2_block_sealer.run());
        let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
        let state_keeper = StateKeeperBuilder::new(
            Box::new(io),
            Box::new(HaltsL1TxsExecutor {
                executed: executed_before.clone(),
                halt_l1_txs: true,
            }),
            OutputHandler::new(Box::new(persistence.with_tx_insertion())),
            Arc::new(NoopSealer),
            Arc::new(MockReadStorageFactory),
            None,
        )
        .build(&stop_receiver)
        .await
        .unwrap();
        let keeper_handle = tokio::spawn(state_keeper.run(stop_receiver));

        let deadline = Instant::now() + Duration::from_secs(30);
        while !guard.l1_transactions_paused() {
            assert!(
                !keeper_handle.is_finished(),
                "the state keeper stopped instead of pausing"
            );
            assert!(Instant::now() < deadline, "the queue never paused");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            executed_before.lock().unwrap().contains(&l1_hash),
            "the deposit was never tried"
        );

        stop_sender.send_replace(true);
        keeper_handle.await.expect("the state keeper panicked").ok();
        sealer_handle.abort();
    }

    // The database still holds the deposit as pending, which is the whole reason a restart can pick
    // it up. `next_priority_id()` is what `MempoolGuard::from_storage()` reads, and it still points
    // at this transaction rather than past it.
    let mut conn = connection_pool.connection().await.unwrap();
    assert_eq!(
        conn.transactions_dal().next_priority_id().await,
        PriorityOpId(0),
        "the priority id advanced past a deposit that never executed"
    );

    // ---- Restart: a fresh mempool, built the production way so the id comes from Postgres. ----
    //
    // `MempoolFetcher::run()` clears `in_mempool` before its first sync, and that step is load
    // bearing rather than housekeeping: `sync_mempool()` hands each transaction out once and leaves
    // it flagged, so without the reset a restarted node would never be offered the deposit again.
    // Nothing clears the flag when a process dies, so the restart has to. Modelled here for the same
    // reason the rest of this test exists — the flag lives in Postgres and outlives the process.
    conn.transactions_dal().reset_mempool().await.unwrap();
    let restarted = MempoolGuard::from_storage(&mut conn, 100, None, None).await;
    drop(conn);
    assert!(
        !restarted.l1_transactions_paused(),
        "the pause survived the restart; it is in-memory state and must not"
    );
    let loaded = load_from_postgres(&connection_pool, &filter).await;
    assert!(
        loaded.iter().any(|(tx, _)| tx.hash() == l1_hash),
        "the deposit was not re-loaded from Postgres after the restart: {loaded:?}"
    );
    restarted.insert(loaded, HashMap::new());

    // ---- Second run: with the cause dealt with, the same deposit executes. ----
    let io = tester
        .create_test_mempool_io_around(
            connection_pool.clone(),
            restarted.clone(),
            WorkingSettlementLayer::for_tests(),
        )
        .await;
    let executed_after = Arc::new(Mutex::new(Vec::new()));
    let (persistence, l2_block_sealer) =
        StateKeeperPersistence::new(connection_pool.clone(), Some(Address::default()), 5)
            .await
            .unwrap();
    let sealer_handle = tokio::spawn(l2_block_sealer.run());
    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let state_keeper = StateKeeperBuilder::new(
        Box::new(io),
        Box::new(HaltsL1TxsExecutor {
            executed: executed_after.clone(),
            halt_l1_txs: false,
        }),
        OutputHandler::new(Box::new(persistence.with_tx_insertion())),
        Arc::new(NoopSealer),
        Arc::new(MockReadStorageFactory),
        None,
    )
    .build(&stop_receiver)
    .await
    .unwrap();
    let keeper_handle = tokio::spawn(state_keeper.run(stop_receiver));

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            !keeper_handle.is_finished(),
            "the state keeper stopped during the second run"
        );
        if executed_after.lock().unwrap().contains(&l1_hash) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the deposit was never executed after the restart; executed: {:?}",
            executed_after.lock().unwrap()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Executed, and the queue is running rather than paused again.
    assert!(
        !restarted.l1_transactions_paused(),
        "the queue paused again on a deposit that executed fine"
    );

    stop_sender.send_replace(true);
    keeper_handle.await.expect("the state keeper panicked").ok();
    sealer_handle.abort();
}

fn priority_tx(serial_id: u64) -> L1Tx {
    L1Tx {
        execute: Execute {
            contract_address: Some(Address::random()),
            calldata: vec![],
            value: U256::zero(),
            factory_deps: vec![],
        },
        common_data: L1TxCommonData {
            sender: Address::random(),
            serial_id: PriorityOpId(serial_id),
            gas_limit: U256::from(100_000),
            gas_per_pubdata_limit: U256::from(800),
            canonical_tx_hash: H256::from_low_u64_be(0x1000 + serial_id),
            ..L1TxCommonData::default()
        },
        received_timestamp_ms: 0,
    }
}

/// Hands the mempool's priority queue to `io` and returns the `force_fail` flag of each tx it hands
/// out, keyed by serial id.
async fn force_fail_flags_handed_out(
    io: &mut MempoolIO,
    guard: &MempoolGuard,
    txs: &[L1Tx],
) -> Vec<(PriorityOpId, bool)> {
    guard.insert(
        txs.iter()
            .map(|tx| {
                (
                    Transaction::from(tx.clone()),
                    TransactionTimeRangeConstraint::default(),
                )
            })
            .collect(),
        HashMap::new(),
    );
    let mut flags = vec![];
    for _ in txs {
        let tx = io
            .wait_for_next_tx(Duration::from_secs(1), 0)
            .await
            .unwrap()
            .expect("no tx handed out");
        let ExecuteTransactionCommon::L1(data) = &tx.common_data else {
            panic!("not an L1 tx: {tx:?}");
        };
        flags.push((data.serial_id, data.force_fail));
    }
    flags
}

#[tokio::test]
async fn force_fail_marks_are_applied_from_the_operator_table() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut tester = Tester::new(L1BatchCommitmentMode::Rollup);
    tester.genesis(&connection_pool).await;
    connection_pool
        .connection()
        .await
        .unwrap()
        .force_fail_dal()
        .insert_force_failed_priority_op(PriorityOpId(1), Some("test"), "tests")
        .await
        .unwrap();
    let txs = [priority_tx(0), priority_tx(1)];

    // Feature off: the mark is never read.
    let (mut io, guard) = tester.create_test_mempool_io(connection_pool.clone()).await;
    io.set_last_batch_protocol_version(ProtocolVersionId::Version34);
    assert_eq!(
        force_fail_flags_handed_out(&mut io, &guard, &txs).await,
        [(PriorityOpId(0), false), (PriorityOpId(1), false)]
    );

    // Feature on: only the marked op is flagged.
    tester.enable_force_fail();
    let (mut io, guard) = tester.create_test_mempool_io(connection_pool.clone()).await;
    io.set_last_batch_protocol_version(ProtocolVersionId::Version34);
    assert_eq!(
        force_fail_flags_handed_out(&mut io, &guard, &txs).await,
        [(PriorityOpId(0), false), (PriorityOpId(1), true)]
    );

    // Feature on, but the batch runs a bootloader that cannot read the bit: the mark is ignored.
    // v33 is the last such version (v32's VM with the DSE-compiled bootloader).
    let (mut io, guard) = tester.create_test_mempool_io(connection_pool.clone()).await;
    io.set_last_batch_protocol_version(ProtocolVersionId::Version33);
    assert_eq!(
        force_fail_flags_handed_out(&mut io, &guard, &txs).await,
        [(PriorityOpId(0), false), (PriorityOpId(1), false)]
    );

    // A stale `true` read back from Postgres (e.g. after a block revert) does not survive
    // without a mark; the operator table is the only source of intent.
    let (mut io, guard) = tester.create_test_mempool_io(connection_pool.clone()).await;
    io.set_last_batch_protocol_version(ProtocolVersionId::Version34);
    let mut stale = priority_tx(0);
    stale.common_data.force_fail = true;
    assert_eq!(
        force_fail_flags_handed_out(&mut io, &guard, &[stale]).await,
        [(PriorityOpId(0), false)]
    );
}
