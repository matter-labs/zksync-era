use std::time::Duration;

use futures::FutureExt;
use multivm::utils::derive_base_fee_and_gas_per_pubdata;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_types::{
    block::{BlockGasCount, MiniblockHasher},
    fee::TransactionExecutionMetrics,
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    tx::ExecutionMetrics,
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, StorageKey, VmEvent,
    H256, U256,
};
use zksync_utils::time::seconds_since_epoch;

use self::tester::Tester;
use crate::{
    state_keeper::{
        io::{MiniblockParams, MiniblockSealer, StateKeeperIO},
        mempool_actor::l2_tx_filter,
        tests::{
            create_execution_result, create_transaction, create_updates_manager,
            default_l1_batch_env, default_system_env, default_vm_block_result, Query,
        },
        updates::{MiniblockSealCommand, MiniblockUpdates, UpdatesManager},
    },
    utils::testonly::prepare_recovery_snapshot,
};

mod tester;

/// Ensure that MempoolIO.filter is correctly initialized right after mempool initialization.
#[tokio::test]
async fn test_filter_initialization() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new();

    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mempool, _) = tester.create_test_mempool_io(connection_pool, 1).await;

    // Upon initialization, the filter should be set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());
}

/// Ensure that MempoolIO.filter is modified correctly if there is a pending batch upon mempool initialization.
#[tokio::test]
async fn test_filter_with_pending_batch() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut tester = Tester::new();
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

    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool, 1).await;
    // Before the mempool knows there is a pending batch, the filter is still set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());

    mempool.load_pending_batch().await.unwrap();
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
#[tokio::test]
async fn test_filter_with_no_pending_batch() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new();
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
    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool, 1).await;
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
        .wait_for_new_batch_params(Duration::from_secs(10))
        .await
        .expect("No batch params in the test mempool");
    assert_eq!(mempool.filter(), &want_filter);
}

async fn test_timestamps_are_distinct(
    connection_pool: ConnectionPool<Core>,
    prev_miniblock_timestamp: u64,
    delay_prev_miniblock_compared_to_batch: bool,
) {
    let mut tester = Tester::new();
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

    let (mut mempool, mut guard) = tester.create_test_mempool_io(connection_pool, 1).await;
    // Insert a transaction to trigger L1 batch creation.
    let tx_filter = l2_tx_filter(
        &tester.create_batch_fee_input_provider().await,
        ProtocolVersionId::latest().into(),
    )
    .await;
    tester.insert_tx(&mut guard, tx_filter.fee_per_gas, tx_filter.gas_per_pubdata);

    let (_, l1_batch_env) = mempool
        .wait_for_new_batch_params(Duration::from_secs(10))
        .await
        .unwrap()
        .expect("No batch params in the test mempool");
    assert!(l1_batch_env.timestamp > prev_miniblock_timestamp);
}

#[tokio::test]
async fn l1_batch_timestamp_basics() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, false).await;
}

#[tokio::test]
async fn l1_batch_timestamp_with_clock_skew() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, false).await;
}

#[tokio::test]
async fn l1_batch_timestamp_respects_prev_miniblock() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, true).await;
}

#[tokio::test]
async fn l1_batch_timestamp_respects_prev_miniblock_with_clock_skew() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, true).await;
}

#[tokio::test]
async fn processing_storage_logs_when_sealing_miniblock() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut miniblock = MiniblockUpdates::new(0, 1, H256::zero(), 1, ProtocolVersionId::latest());

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
        miniblock_number: MiniblockNumber(3),
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
    let mut miniblock = MiniblockUpdates::new(0, 1, H256::zero(), 1, ProtocolVersionId::latest());

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

    let miniblock_number = MiniblockNumber(3);
    let seal_command = MiniblockSealCommand {
        l1_batch_number,
        miniblock_number,
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
        .get_all_logs(miniblock_number - 1)
        .await
        .unwrap();

    assert_eq!(logs.len(), 10);
    // The event logs should be inserted in the correct order.
    for (i, log) in logs.iter().enumerate() {
        assert_eq!(log.data.0, [i as u8]);
    }
}

async fn test_miniblock_and_l1_batch_processing(
    pool: ConnectionPool<Core>,
    miniblock_sealer_capacity: usize,
) {
    let tester = Tester::new();

    // Genesis is needed for proper mempool initialization.
    tester.genesis(&pool).await;
    let mut storage = pool.connection().await.unwrap();
    // Save metadata for the genesis L1 batch so that we don't hang in `seal_l1_batch`.
    storage
        .blocks_dal()
        .set_l1_batch_hash(L1BatchNumber(0), H256::zero())
        .await
        .unwrap();
    drop(storage);

    let (mut mempool, _) = tester
        .create_test_mempool_io(pool.clone(), miniblock_sealer_capacity)
        .await;

    let l1_batch_env = default_l1_batch_env(1, 1, Address::random());
    let mut updates = UpdatesManager::new(&l1_batch_env, &default_system_env());

    let tx = create_transaction(10, 100);
    updates.extend_from_executed_transaction(
        tx,
        create_execution_result(0, []),
        vec![],
        BlockGasCount::default(),
        ExecutionMetrics::default(),
        vec![],
    );
    mempool.seal_miniblock(&updates).await;
    updates.push_miniblock(MiniblockParams {
        timestamp: 1,
        virtual_blocks: 1,
    });

    let finished_batch = default_vm_block_result();
    mempool
        .seal_l1_batch(None, updates, &l1_batch_env, finished_batch)
        .await
        .unwrap();

    // Check that miniblock #1 and L1 batch #1 are persisted.
    let mut conn = pool.connection().await.unwrap();
    assert_eq!(
        conn.blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .unwrap(),
        Some(MiniblockNumber(2)) // + fictive miniblock
    );
    let l1_batch_header = conn
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("No L1 batch #1");
    assert_eq!(l1_batch_header.l2_tx_count, 1);
}

#[tokio::test]
async fn miniblock_and_l1_batch_processing() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    test_miniblock_and_l1_batch_processing(pool, 1).await;
}

#[tokio::test]
async fn miniblock_and_l1_batch_processing_with_sync_sealer() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    test_miniblock_and_l1_batch_processing(pool, 0).await;
}

#[tokio::test]
async fn miniblock_processing_after_snapshot_recovery() {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = connection_pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;
    let tester = Tester::new();

    let (mut mempool, mut mempool_guard) = tester
        .create_test_mempool_io(connection_pool.clone(), 0)
        .await;
    assert_eq!(
        mempool.current_miniblock_number(),
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(
        mempool.current_l1_batch_number(),
        snapshot_recovery.l1_batch_number + 1
    );
    assert!(mempool.load_pending_batch().await.unwrap().is_none());

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

    let (system_env, l1_batch_env) = mempool
        .wait_for_new_batch_params(Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no batch params generated");
    assert_eq!(l1_batch_env.number, snapshot_recovery.l1_batch_number + 1);
    assert_eq!(
        l1_batch_env.previous_batch_hash,
        Some(snapshot_recovery.l1_batch_root_hash)
    );
    assert_eq!(
        l1_batch_env.first_l2_block.prev_block_hash,
        snapshot_recovery.miniblock_hash
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
    mempool.seal_miniblock(&updates).await;

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
    let (mut mempool, _) = tester
        .create_test_mempool_io(connection_pool.clone(), 0)
        .await;
    assert_eq!(
        mempool.current_miniblock_number(),
        snapshot_recovery.miniblock_number + 2
    );
    assert_eq!(
        mempool.current_l1_batch_number(),
        snapshot_recovery.l1_batch_number + 1
    );

    let pending_batch = mempool
        .load_pending_batch()
        .await
        .unwrap()
        .expect("no pending batch");
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

#[tokio::test]
async fn miniblock_sealer_handle_blocking() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let (mut sealer, mut sealer_handle) = MiniblockSealer::new(pool, 1);

    // The first command should be successfully submitted immediately.
    let updates_manager = create_updates_manager();
    let seal_command = updates_manager.seal_miniblock_command(
        L1BatchNumber(1),
        MiniblockNumber(1),
        Address::default(),
        false,
    );
    sealer_handle.submit(seal_command).await;

    // The second command should lead to blocking
    let seal_command = updates_manager.seal_miniblock_command(
        L1BatchNumber(1),
        MiniblockNumber(2),
        Address::default(),
        false,
    );
    {
        let submit_future = sealer_handle.submit(seal_command);
        futures::pin_mut!(submit_future);

        assert!((&mut submit_future).now_or_never().is_none());
        // ...until miniblock #1 is processed
        let command = sealer.commands_receiver.recv().await.unwrap();
        command.completion_sender.send(()).unwrap_err(); // completion receiver should be dropped
        submit_future.await;
    }

    {
        let wait_future = sealer_handle.wait_for_all_commands();
        futures::pin_mut!(wait_future);
        assert!((&mut wait_future).now_or_never().is_none());
        let command = sealer.commands_receiver.recv().await.unwrap();
        command.completion_sender.send(()).unwrap();
        wait_future.await;
    }

    // Check that `wait_for_all_commands()` state is reset after use.
    sealer_handle.wait_for_all_commands().await;

    let seal_command = updates_manager.seal_miniblock_command(
        L1BatchNumber(2),
        MiniblockNumber(3),
        Address::default(),
        false,
    );
    sealer_handle.submit(seal_command).await;
    let command = sealer.commands_receiver.recv().await.unwrap();
    command.completion_sender.send(()).unwrap();
    sealer_handle.wait_for_all_commands().await;
}

#[tokio::test]
async fn miniblock_sealer_handle_parallel_processing() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let (mut sealer, mut sealer_handle) = MiniblockSealer::new(pool, 5);

    // 5 miniblock sealing commands can be submitted without blocking.
    for i in 1..=5 {
        let updates_manager = create_updates_manager();
        let seal_command = updates_manager.seal_miniblock_command(
            L1BatchNumber(1),
            MiniblockNumber(i),
            Address::default(),
            false,
        );
        sealer_handle.submit(seal_command).await;
    }

    for i in 1..=5 {
        let command = sealer.commands_receiver.recv().await.unwrap();
        assert_eq!(command.command.miniblock_number, MiniblockNumber(i));
        command.completion_sender.send(()).ok();
    }

    sealer_handle.wait_for_all_commands().await;
}

/// Ensure that subsequent miniblocks that belong to the same L1 batch have different timestamps
#[tokio::test]
async fn different_timestamp_for_miniblocks_in_same_batch() {
    let connection_pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tester = Tester::new();

    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mut mempool, _) = tester.create_test_mempool_io(connection_pool, 1).await;
    let current_timestamp = seconds_since_epoch();
    mempool.set_prev_miniblock_timestamp(current_timestamp);

    let miniblock_params = mempool
        .wait_for_new_miniblock_params(Duration::from_secs(10))
        .await
        .unwrap()
        .expect("no new miniblock params");
    assert!(miniblock_params.timestamp > current_timestamp);
}
