use std::{sync::Arc, time::Duration};

use futures::FutureExt;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::ConnectionPool;
use zksync_types::{
    block::BlockGasCount,
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    tx::ExecutionMetrics,
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, StorageKey, VmEvent,
    H256, U256,
};
use zksync_utils::time::seconds_since_epoch;

use self::tester::Tester;
use crate::{
    l1_gas_price::{RollupPubdataPricing, ValidiumPubdataPricing},
    state_keeper::{
        io::{MiniblockParams, MiniblockSealer, StateKeeperIO},
        mempool_actor::l2_tx_filter,
        tests::{
            create_execution_result, create_transaction, create_updates_manager,
            default_l1_batch_env, default_system_env, default_vm_block_result, Query,
        },
        updates::{MiniblockSealCommand, MiniblockUpdates, UpdatesManager},
    },
};

mod tester;
mod tests_helpers;

/// Ensure that MempoolIO.filter is correctly initialized right after mempool initialization.
#[tokio::test]
async fn test_filter_initialization() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));

    tests_helpers::test_filter_initialization(rollup_tester).await;
    tests_helpers::test_filter_initialization(validium_tester).await;
}

/// Ensure that MempoolIO.filter is modified correctly if there is a pending batch upon mempool initialization.
#[tokio::test]
async fn test_filter_with_pending_batch() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));

    tests_helpers::test_filter_with_pending_batch(rollup_tester).await;
    tests_helpers::test_filter_with_pending_batch(validium_tester).await;
}

/// Ensure that `MempoolIO.filter` is modified correctly if there is no pending batch.
#[tokio::test]
async fn test_filter_with_no_pending_batch() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));

    tests_helpers::test_filter_with_no_pending_batch(rollup_tester).await;
    tests_helpers::test_filter_with_no_pending_batch(validium_tester).await;
}

async fn test_timestamps_are_distinct(
    connection_pool: ConnectionPool,
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
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, false, rollup_tester).await;

    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, false, validium_tester).await;
}

#[tokio::test]
async fn l1_batch_timestamp_with_clock_skew() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, false, rollup_tester)
        .await;

    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(
        connection_pool,
        current_timestamp + 2,
        false,
        validium_tester,
    )
    .await;
}

#[tokio::test]
async fn l1_batch_timestamp_respects_prev_miniblock() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, true, rollup_tester).await;

    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp, true, validium_tester).await;
}

#[tokio::test]
async fn l1_batch_timestamp_respects_prev_miniblock_with_clock_skew() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(connection_pool, current_timestamp + 2, true, rollup_tester).await;

    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    let current_timestamp = seconds_since_epoch();
    test_timestamps_are_distinct(
        connection_pool,
        current_timestamp + 2,
        true,
        validium_tester,
    )
    .await;
}

#[tokio::test]
async fn processing_storage_logs_when_sealing_miniblock() {
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
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
    let mut conn = connection_pool.access_storage().await.unwrap();
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
    let pool = ConnectionPool::constrained_test_pool(1).await;
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
    let mut conn = pool.access_storage().await.unwrap();
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
    pool: ConnectionPool,
    miniblock_sealer_capacity: usize,
    tester: Tester,
) {
    // Genesis is needed for proper mempool initialization.
    tester.genesis(&pool).await;
    let mut storage = pool.access_storage().await.unwrap();
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
    let mut conn = pool.access_storage().await.unwrap();
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
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let pool = ConnectionPool::constrained_test_pool(1).await;
    test_miniblock_and_l1_batch_processing(pool, 1, rollup_tester).await;

    let validium_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let pool = ConnectionPool::constrained_test_pool(1).await;
    test_miniblock_and_l1_batch_processing(pool, 1, validium_tester).await;
}

#[tokio::test]
async fn miniblock_and_l1_batch_processing_with_sync_sealer() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let pool = ConnectionPool::constrained_test_pool(1).await;
    test_miniblock_and_l1_batch_processing(pool, 0, rollup_tester).await;

    let validium_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let pool = ConnectionPool::constrained_test_pool(1).await;
    test_miniblock_and_l1_batch_processing(pool, 0, validium_tester).await;
}

#[tokio::test]
async fn miniblock_processing_after_snapshot_recovery() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));

    tests_helpers::miniblock_processing_after_snapshot_recovery(rollup_tester).await;
    tests_helpers::miniblock_processing_after_snapshot_recovery(validium_tester).await;
}
#[tokio::test]
async fn miniblock_sealer_handle_blocking() {
    let pool = ConnectionPool::constrained_test_pool(1).await;
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
    let pool = ConnectionPool::constrained_test_pool(1).await;
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

#[tokio::test]
async fn different_timestamp_for_miniblocks_in_same_batch() {
    let rollup_tester = Tester::new(Arc::new(RollupPubdataPricing {}));
    let validium_tester = Tester::new(Arc::new(ValidiumPubdataPricing {}));

    tests_helpers::different_timestamp_for_miniblocks_in_same_batch(rollup_tester).await;
    tests_helpers::different_timestamp_for_miniblocks_in_same_batch(validium_tester).await;
}
