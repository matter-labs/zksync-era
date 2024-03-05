use std::time::Duration;

use multivm::utils::derive_base_fee_and_gas_per_pubdata;
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;
use zksync_types::{
    block::{BlockGasCount, MiniblockHasher},
    fee::TransactionExecutionMetrics,
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    tx::ExecutionMetrics,
    L1BatchNumber, MiniblockNumber, ProtocolVersionId,
};
use zksync_utils::time::seconds_since_epoch;

use super::tester::Tester;
use crate::{
    state_keeper::{
        mempool_actor::l2_tx_filter, tests::create_execution_result, updates::UpdatesManager,
        StateKeeperIO,
    },
    utils::testonly::prepare_recovery_snapshot,
};

/// Ensure that MempoolIO.filter is correctly initialized right after mempool initialization.
pub(crate) async fn test_filter_initialization(tester: Tester) {
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool).await;
    let (mempool, _) = tester.create_test_mempool_io(connection_pool, 1).await;

    // Upon initialization, the filter should be set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());
}

/// Ensure that MempoolIO.filter is modified correctly if there is a pending batch upon mempool initialization.
pub(crate) async fn test_filter_with_pending_batch(mut tester: Tester) {
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
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
pub(crate) async fn test_filter_with_no_pending_batch(tester: Tester) {
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;
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

pub(crate) async fn miniblock_processing_after_snapshot_recovery(tester: Tester) {
    let connection_pool = ConnectionPool::test_pool().await;
    let mut storage = connection_pool.access_storage().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

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
        .await;

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

/// Ensure that subsequent miniblocks that belong to the same L1 batch have different timestamps
pub(crate) async fn different_timestamp_for_miniblocks_in_same_batch(tester: Tester) {
    let connection_pool = ConnectionPool::constrained_test_pool(1).await;

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
