use std::time::{Duration, SystemTime, UNIX_EPOCH};

use test_casing::test_casing;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_multivm::{
    interface::{
        tracer::ValidationTraces, TransactionExecutionMetrics, VmEvent, VmExecutionMetrics,
    },
    utils::derive_base_fee_and_gas_per_pubdata,
};
use zksync_node_test_utils::prepare_recovery_snapshot;
use zksync_system_constants::KNOWN_CODES_STORAGE_ADDRESS;
use zksync_types::{
    block::L2BlockHasher,
    bytecode::BytecodeHash,
    commitment::{L1BatchCommitmentMode, PubdataParams},
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    l2::L2Tx,
    protocol_upgrade::ProtocolUpgradeTx,
    protocol_version::ProtocolSemanticVersion,
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersion,
    ProtocolVersionId, StorageKey, TransactionTimeRangeConstraint, H256, U256,
};

use self::tester::Tester;
use crate::{
    io::{seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, StateKeeperIO},
    mempool_actor::l2_tx_filter,
    testonly::BASE_SYSTEM_CONTRACTS,
    tests::{create_execution_result, create_transaction, seconds_since_epoch, Query},
    updates::{L2BlockSealCommand, L2BlockUpdates, UpdatesManager},
    StateKeeperOutputHandler, StateKeeperPersistence,
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
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;

    // Inserting a pending L2 block that isn't included in a sealed batch means there is a pending batch.
    // The gas values are randomly chosen but so affect filter values calculation.

    let fee_input = BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
        l1_gas_price: 100,
        fair_l2_gas_price: 1000,
        fair_pubdata_price: 500,
    });
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
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;

    // Create a copy of the tx filter that the mempool will use.
    let want_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest().into(),
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

async fn test_timestamps_are_distinct(
    connection_pool: ConnectionPool<Core>,
    prev_l2_block_timestamp: u64,
    delay_prev_l2_block_compared_to_batch: bool,
    mut tester: Tester,
) {
    tester.genesis(&connection_pool).await;

    tester.set_timestamp(prev_l2_block_timestamp);
    let tx_result = tester
        .insert_l2_block(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    if delay_prev_l2_block_compared_to_batch {
        tester.set_timestamp(prev_l2_block_timestamp - 1);
    }
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;

    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;
    let (io_cursor, _) = mempool.initialize().await.unwrap();
    // Insert a transaction to trigger L1 batch creation.
    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest().into(),
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
    assert!(l1_batch_params.first_l2_block.timestamp > prev_l2_block_timestamp);
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_basics(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, false, tester).await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_with_clock_skew(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, false, tester).await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_respects_prev_l2_block(commitment_mode: L1BatchCommitmentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, true, tester).await;
}

#[test_casing(2, COMMITMENT_MODES)]
#[tokio::test]
async fn l1_batch_timestamp_respects_prev_l2_block_with_clock_skew(
    commitment_mode: L1BatchCommitmentMode,
) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(commitment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, true, tester).await;
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
        pre_insert_txs: false,
        pubdata_params: PubdataParams::default(),
        rolling_txs_hash: Default::default(),
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
    let seal_command = create_block_seal_command(l1_batch_number, l2_block);
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
        ProtocolVersionId::latest().into(),
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
    let (system_env, l1_batch_env, pubdata_params) = l1_batch_params.into_env(
        L2ChainId::default(),
        BASE_SYSTEM_CONTRACTS.clone(),
        &cursor,
        previous_batch_hash,
    );
    let mut updates = UpdatesManager::new(
        &l1_batch_env,
        &system_env,
        pubdata_params,
        system_env.version,
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
    persistence.handle_l2_block(&updates).await.unwrap();

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

/// Ensure that subsequent L2 blocks that belong to the same L1 batch have different timestamps
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

    let l2_block_params = mempool
        .wait_for_new_l2_block_params(&io_cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no new L2 block params");
    assert!(l2_block_params.timestamp > current_timestamp);
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
        ProtocolVersionId::latest().into(),
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

    let new_l1_batch_params = mempool
        .wait_for_new_batch_params(&cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no batch params generated");

    assert_eq!(old_l1_batch_params, new_l1_batch_params);
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
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;
    // Pre-insert L2 block without its unsealed L1 batch counterpart
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
    assert_eq!(l1_batch_params.first_l2_block.timestamp, 2);
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
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;

    // Create a copy of the tx filter that the mempool will use.
    let want_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest().into(),
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
