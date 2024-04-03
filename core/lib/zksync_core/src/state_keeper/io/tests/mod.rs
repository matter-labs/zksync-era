use std::time::Duration;

use multivm::utils::derive_base_fee_and_gas_per_pubdata;
use test_casing::test_casing;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_types::{
    block::{BlockGasCount, MiniblockHasher},
    fee::TransactionExecutionMetrics,
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    tx::ExecutionMetrics,
    AccountTreeId, Address, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId,
    StorageKey, VmEvent, H256, U256,
};
use zksync_utils::time::seconds_since_epoch;

use self::tester::Tester;
use crate::{
    state_keeper::{
        io::StateKeeperIO,
        mempool_actor::l2_tx_filter,
        tests::{create_execution_result, create_transaction, Query, BASE_SYSTEM_CONTRACTS},
        updates::{MiniblockSealCommand, MiniblockUpdates, UpdatesManager},
        StateKeeperOutputHandler, StateKeeperPersistence,
    },
    utils::testonly::{prepare_recovery_snapshot, DeploymentMode},
};

mod tester;

/// Ensure that MempoolIO.filter is correctly initialized right after mempool initialization.
#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn test_filter_initialization(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);
    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mempool, _) = tester.create_test_mempool_io(connection_pool).await;

    // Upon initialization, the filter should be set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());
}

/// Ensure that MempoolIO.filter is modified correctly if there is a pending batch upon mempool initialization.
#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn test_filter_with_pending_batch(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut tester = Tester::new(&deployment_mode);
    tester.genesis(&connection_pool).await;

    // Insert a sealed batch so there will be a `prev_l1_batch_state_root`.
    // These gas values are random and don't matter for filter calculation as there will be a
    // pending batch the filter will be based off of.
    let tx_result = tester
        .insert_miniblock(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;

    // Inserting a pending miniblock that isn't included in a sealed batch means there is a pending batch.
    // The gas values are randomly chosen but so affect filter values calculation.

    let fee_input = BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
        l1_gas_price: 100,
        fair_l2_gas_price: 1000,
        fair_pubdata_price: 500,
    });
    tester.set_timestamp(2);
    tester
        .insert_miniblock(&connection_pool, 2, 10, fee_input)
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
#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn test_filter_with_no_pending_batch(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);
    tester.genesis(&connection_pool).await;

    // Insert a sealed batch so there will be a `prev_l1_batch_state_root`.
    // These gas values are random and don't matter for filter calculation.
    let tx_result = tester
        .insert_miniblock(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    tester
        .insert_sealed_batch(&connection_pool, 1, &[tx_result])
        .await;

    // Create a copy of the tx filter that the mempool will use.
    let want_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest().into(),
    )
    .await;

    // Create a mempool without pending batch and ensure that filter is not initialized just yet.
    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool).await;
    let (io_cursor, _) = mempool.initialize().await.unwrap();
    assert_eq!(mempool.filter(), &L2TxFilter::default());

    // Insert a transaction that matches the expected filter.
    tester.insert_tx(
        &mut guard,
        want_filter.fee_per_gas,
        want_filter.gas_per_pubdata,
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
    prev_miniblock_timestamp: u64,
    delay_prev_miniblock_compared_to_batch: bool,
    mut tester: Tester,
) {
    tester.genesis(&connection_pool).await;

    tester.set_timestamp(prev_miniblock_timestamp);
    let tx_result = tester
        .insert_miniblock(&connection_pool, 1, 5, BatchFeeInput::l1_pegged(55, 555))
        .await;
    if delay_prev_miniblock_compared_to_batch {
        tester.set_timestamp(prev_miniblock_timestamp - 1);
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
    .await;
    tester.insert_tx(&mut guard, tx_filter.fee_per_gas, tx_filter.gas_per_pubdata);

    let l1_batch_params = mempool
        .wait_for_new_batch_params(&io_cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("No batch params in the test mempool");
    assert!(l1_batch_params.first_miniblock.timestamp > prev_miniblock_timestamp);
}

#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn l1_batch_timestamp_basics(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, false, tester).await;
}

#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn l1_batch_timestamp_with_clock_skew(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, false, tester).await;
}

#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn l1_batch_timestamp_respects_prev_miniblock(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, true, tester).await;
}

#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn l1_batch_timestamp_respects_prev_miniblock_with_clock_skew(
    deployment_mode: DeploymentMode,
) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, true, tester).await;
}

#[tokio::test]
async fn processing_storage_logs_when_sealing_miniblock() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut miniblock = MiniblockUpdates::new(
        0,
        MiniblockNumber(3),
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
    let execution_result = create_execution_result(0, storage_logs);
    miniblock.extend_from_executed_transaction(
        tx,
        execution_result,
        BlockGasCount::default(),
        ExecutionMetrics::default(),
        vec![],
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
    let execution_result = create_execution_result(1, storage_logs);
    miniblock.extend_from_executed_transaction(
        tx,
        execution_result,
        BlockGasCount::default(),
        ExecutionMetrics::default(),
        vec![],
        vec![],
    );

    let l1_batch_number = L1BatchNumber(2);
    let seal_command = MiniblockSealCommand {
        l1_batch_number,
        miniblock,
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
        l2_erc20_bridge_addr: Address::default(),
        pre_insert_txs: false,
    };
    let mut conn = connection_pool.connection().await.unwrap();
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(Default::default())
        .await;
    seal_command.seal(&mut conn).await;

    // Manually mark the miniblock as executed so that getting touched slots from it works
    conn.blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(l1_batch_number)
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
    assert!(!touched_slots.contains_key(&read_key));

    // The storage logs must be inserted and read in the correct order, so that
    // `touched_slots` contain the most recent values in the L1 batch.
    assert_eq!(touched_slots.len(), 3);
    let written_kvs = [(2, 4), (3, 6), (4, 5)];
    for (key, value) in written_kvs {
        let key = StorageKey::new(account, H256::from_low_u64_be(key));
        let expected_value = H256::from_low_u64_be(value);
        assert_eq!(touched_slots[&key], expected_value);
    }
}

#[tokio::test]
async fn processing_events_when_sealing_miniblock() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let l1_batch_number = L1BatchNumber(2);
    let mut miniblock = MiniblockUpdates::new(
        0,
        MiniblockNumber(3),
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

    for (i, events_chunk) in events.chunks(4).enumerate() {
        let tx = create_transaction(10, 100);
        let mut execution_result = create_execution_result(i as u16, []);
        execution_result.logs.events = events_chunk.to_vec();
        miniblock.extend_from_executed_transaction(
            tx,
            execution_result,
            BlockGasCount::default(),
            ExecutionMetrics::default(),
            vec![],
            vec![],
        );
    }

    let seal_command = MiniblockSealCommand {
        l1_batch_number,
        miniblock,
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
        l2_erc20_bridge_addr: Address::default(),
        pre_insert_txs: false,
    };
    let mut conn = pool.connection().await.unwrap();
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(Default::default())
        .await;
    seal_command.seal(&mut conn).await;

    let logs = conn
        .events_web3_dal()
        .get_all_logs(seal_command.miniblock.number - 1)
        .await
        .unwrap();

    assert_eq!(logs.len(), 10);
    // The event logs should be inserted in the correct order.
    for (i, log) in logs.iter().enumerate() {
        assert_eq!(log.data.0, [i as u8]);
    }
}

#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn miniblock_processing_after_snapshot_recovery(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let tester = Tester::new(&deployment_mode);
    let mut storage = connection_pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

    let (mut mempool, mut mempool_guard) =
        tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, maybe_pending_batch) = mempool.initialize().await.unwrap();
    assert_eq!(
        cursor.next_miniblock,
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(cursor.l1_batch, snapshot_recovery.l1_batch_number + 1);
    assert!(maybe_pending_batch.is_none());

    // Insert a transaction into the mempool in order to open a new batch.
    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest().into(),
    )
    .await;
    let tx = tester.insert_tx(
        &mut mempool_guard,
        tx_filter.fee_per_gas,
        tx_filter.gas_per_pubdata,
    );
    storage
        .transactions_dal()
        .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
        .await
        .unwrap();

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
    let (system_env, l1_batch_env) = l1_batch_params.into_env(
        L2ChainId::default(),
        BASE_SYSTEM_CONTRACTS.clone(),
        &cursor,
        previous_batch_hash,
    );
    let mut updates = UpdatesManager::new(&l1_batch_env, &system_env);

    let tx_hash = tx.hash();
    updates.extend_from_executed_transaction(
        tx.into(),
        create_execution_result(0, []),
        vec![],
        BlockGasCount::default(),
        ExecutionMetrics::default(),
        vec![],
    );

    let (mut persistence, miniblock_sealer) =
        StateKeeperPersistence::new(connection_pool.clone(), Address::default(), 0);
    tokio::spawn(miniblock_sealer.run());
    persistence.handle_miniblock(&updates).await.unwrap();

    // Check that the miniblock is persisted and has correct data.
    let persisted_miniblock = storage
        .blocks_dal()
        .get_miniblock_header(snapshot_recovery.miniblock_number + 1)
        .await
        .unwrap()
        .expect("no miniblock persisted");
    assert_eq!(
        persisted_miniblock.number,
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(persisted_miniblock.l2_tx_count, 1);

    let mut miniblock_hasher = MiniblockHasher::new(
        persisted_miniblock.number,
        persisted_miniblock.timestamp,
        snapshot_recovery.miniblock_hash,
    );
    miniblock_hasher.push_tx_hash(tx_hash);
    assert_eq!(
        persisted_miniblock.hash,
        miniblock_hasher.finalize(ProtocolVersionId::latest())
    );

    let miniblock_transactions = storage
        .transactions_web3_dal()
        .get_raw_miniblock_transactions(persisted_miniblock.number)
        .await
        .unwrap();
    assert_eq!(miniblock_transactions.len(), 1);
    assert_eq!(miniblock_transactions[0].hash(), tx_hash);

    // Emulate node restart.
    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool.clone()).await;
    let (cursor, maybe_pending_batch) = mempool.initialize().await.unwrap();
    assert_eq!(
        cursor.next_miniblock,
        snapshot_recovery.miniblock_number + 2
    );
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
        snapshot_recovery.miniblock_hash
    );
    assert_eq!(pending_batch.pending_miniblocks.len(), 1);
    assert_eq!(
        pending_batch.pending_miniblocks[0].number,
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(
        pending_batch.pending_miniblocks[0].prev_block_hash,
        snapshot_recovery.miniblock_hash
    );
    assert_eq!(pending_batch.pending_miniblocks[0].txs.len(), 1);
    assert_eq!(pending_batch.pending_miniblocks[0].txs[0].hash(), tx_hash);
}

/// Ensure that subsequent miniblocks that belong to the same L1 batch have different timestamps
#[test_casing(2, [DeploymentMode::Rollup, DeploymentMode::Validium])]
#[tokio::test]
async fn different_timestamp_for_miniblocks_in_same_batch(deployment_mode: DeploymentMode) {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new(&deployment_mode);

    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool).await;
    let (mut io_cursor, _) = mempool.initialize().await.unwrap();
    let current_timestamp = seconds_since_epoch();
    io_cursor.prev_miniblock_timestamp = current_timestamp;

    let miniblock_params = mempool
        .wait_for_new_miniblock_params(&io_cursor, Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no new miniblock params");
    assert!(miniblock_params.timestamp > current_timestamp);
}
