use std::time::Duration;

use db_test_macro::db_test;
use vm::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata;
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;

use self::tester::Tester;
use crate::state_keeper::{io::StateKeeperIO, mempool_actor::l2_tx_filter};

mod tester;

/// Ensure that MempoolIO.filter is correctly initialized right after mempool initialization.
#[db_test]
async fn test_filter_initialization(connection_pool: ConnectionPool) {
    let tester = Tester::new();

    // Genesis is needed for proper mempool initialization.
    tester.genesis(&connection_pool);

    let (mempool, _) = tester
        .create_test_mempool_io(connection_pool)
        .await
        .unwrap();

    // Upon initialization, the filter should be set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());
}

/// Ensure that MempoolIO.filter is modified correctly if there is a pending batch upon mempool initialization.
#[db_test]
async fn test_filter_with_pending_batch(connection_pool: ConnectionPool) {
    let tester = Tester::new();

    tester.genesis(&connection_pool);

    // Insert a sealed batch so there will be a prev_l1_batch_state_root.
    // These gas values are random and don't matter for filter calculation as there will be a
    // pending batch the filter will be based off of.
    tester.insert_miniblock(&connection_pool, 1, 5, 55, 555);

    tester.insert_sealed_batch(&connection_pool, 1);

    // Inserting a pending miniblock that isn't included in a sealed batch means there is a pending batch.
    // The gas values are randomly chosen but so affect filter values calculation.
    let (give_l1_gas_price, give_fair_l2_gas_price) = (100, 1000);
    tester.insert_miniblock(
        &connection_pool,
        2,
        10,
        give_l1_gas_price,
        give_fair_l2_gas_price,
    );

    let (mut mempool, _) = tester
        .create_test_mempool_io(connection_pool)
        .await
        .unwrap();

    // Before the mempool knows there is a pending batch, the filter is still set to the default values.
    assert_eq!(mempool.filter(), &L2TxFilter::default());

    mempool.load_pending_batch();

    let (want_base_fee, want_gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(give_l1_gas_price, give_fair_l2_gas_price);

    let want_filter = L2TxFilter {
        l1_gas_price: give_l1_gas_price,
        fee_per_gas: want_base_fee,
        gas_per_pubdata: want_gas_per_pubdata as u32,
    };

    assert_eq!(mempool.filter(), &want_filter);
}

/// Ensure that MempoolIO.filter is modified correctly if there is no pending batch.
#[db_test]
async fn test_filter_with_no_pending_batch(connection_pool: ConnectionPool) {
    let tester = Tester::new();
    tester.genesis(&connection_pool);

    // Insert a sealed batch so there will be a prev_l1_batch_state_root.
    // These gas values are random and don't matter for filter calculation.
    tester.insert_miniblock(&connection_pool, 1, 5, 55, 555);
    tester.insert_sealed_batch(&connection_pool, 1);

    // Create a copy of the tx filter that the mempool will use.
    let want_filter = l2_tx_filter(
        &tester.create_gas_adjuster().await,
        tester.fair_l2_gas_price(),
    );

    // Create a mempool without pending batch and ensure that filter is not initialized just yet.
    let (mut mempool, mut guard) = tester
        .create_test_mempool_io(connection_pool)
        .await
        .unwrap();
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
        .expect("No batch params in the test mempool");
    assert_eq!(mempool.filter(), &want_filter);
}
