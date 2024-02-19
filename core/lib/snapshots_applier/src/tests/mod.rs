//! Snapshot applier tests.

use std::sync::atomic::{AtomicUsize, Ordering};

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    Address, L1BatchNumber, ProtocolVersion, ProtocolVersionId,
};

use self::utils::{
    mock_recovery_status, prepare_clients, MockMainNodeClient, ObjectStoreWithErrors,
};
use super::*;

mod utils;

#[test_casing(3, [(None, false), (Some(2), false), (None, true)])]
#[tokio::test]
async fn snapshots_creator_can_successfully_recover_db(
    pool_size: Option<u32>,
    with_object_store_errors: bool,
) {
    let pool = if let Some(pool_size) = pool_size {
        ConnectionPool::constrained_test_pool(pool_size).await
    } else {
        ConnectionPool::test_pool().await
    };
    let expected_status = mock_recovery_status();
    let (object_store, client, all_snapshot_storage_logs) = prepare_clients(&expected_status).await;

    let object_store_with_errors;
    let object_store: &dyn ObjectStore = if with_object_store_errors {
        let error_counter = AtomicUsize::new(0);
        object_store_with_errors = ObjectStoreWithErrors::new(object_store, move |_| {
            if error_counter.fetch_add(1, Ordering::SeqCst) >= 3 {
                Ok(()) // "recover" after 3 retries
            } else {
                Err(ObjectStoreError::Other("transient error".into()))
            }
        });
        &object_store_with_errors
    } else {
        &object_store
    };

    let outcome = SnapshotsApplierConfig::for_tests()
        .run(&pool, &client, object_store)
        .await
        .unwrap();
    assert_matches!(outcome, SnapshotsApplierOutcome::Ok);

    let mut storage = pool.access_storage().await.unwrap();
    let mut recovery_dal = storage.snapshot_recovery_dal();

    let current_db_status = recovery_dal.get_applied_snapshot_status().await.unwrap();
    assert_eq!(current_db_status.unwrap(), expected_status);

    let all_initial_writes = storage
        .storage_logs_dedup_dal()
        .dump_all_initial_writes_for_tests()
        .await;
    assert_eq!(all_initial_writes.len(), all_snapshot_storage_logs.len());
    for initial_write in all_initial_writes {
        let log = &all_snapshot_storage_logs[&initial_write.hashed_key];
        assert_eq!(
            initial_write.l1_batch_number,
            log.l1_batch_number_of_initial_write
        );
        assert_eq!(initial_write.index, log.enumeration_index);
    }

    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert_eq!(all_storage_logs.len(), all_snapshot_storage_logs.len());
    for db_log in all_storage_logs {
        let expected_log = &all_snapshot_storage_logs[&db_log.hashed_key];
        assert_eq!(db_log.address, *expected_log.key.address());
        assert_eq!(db_log.key, *expected_log.key.key());
        assert_eq!(db_log.value, expected_log.value);
        assert_eq!(db_log.miniblock_number, expected_status.miniblock_number);
    }

    // Try recovering again; it should return early.
    let err = SnapshotsApplier::load_snapshot(&pool, &client, object_store)
        .await
        .unwrap_err();
    assert_matches!(
        err,
        SnapshotsApplierError::EarlyReturn(SnapshotsApplierOutcome::Ok)
    );
}

#[tokio::test]
async fn applier_returns_early_after_genesis() {
    let pool = ConnectionPool::test_pool().await;

    // We don't want to depend on the core crate, so instead we cheaply emulate it.
    let mut storage = pool.access_storage().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;
    let genesis_miniblock = MiniblockHeader {
        number: MiniblockNumber(0),
        timestamp: 0,
        hash: H256::zero(),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::repeat_byte(1),
        base_fee_per_gas: 1,
        batch_fee_input: Default::default(),
        gas_per_pubdata_limit: 2,
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 0,
    };
    storage
        .blocks_dal()
        .insert_miniblock(&genesis_miniblock)
        .await
        .unwrap();
    let genesis_l1_batch = L1BatchHeader::new(
        L1BatchNumber(0),
        0,
        Default::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&genesis_l1_batch)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await
        .unwrap();

    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = MockMainNodeClient::default();

    let outcome = SnapshotsApplierConfig::for_tests()
        .run(&pool, &client, &object_store)
        .await
        .unwrap();
    assert_matches!(outcome, SnapshotsApplierOutcome::InitializedWithoutSnapshot);
}

#[tokio::test]
async fn applier_returns_early_without_snapshots() {
    let pool = ConnectionPool::test_pool().await;
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = MockMainNodeClient::default();

    let outcome = SnapshotsApplierConfig::for_tests()
        .run(&pool, &client, &object_store)
        .await
        .unwrap();
    assert_matches!(outcome, SnapshotsApplierOutcome::NoSnapshotsOnMainNode);
}

#[tokio::test]
async fn applier_returns_error_on_fatal_object_store_error() {
    let pool = ConnectionPool::test_pool().await;
    let expected_status = mock_recovery_status();
    let (object_store, client, _) = prepare_clients(&expected_status).await;
    let object_store = ObjectStoreWithErrors::new(object_store, |_| {
        Err(ObjectStoreError::KeyNotFound("not found".into()))
    });

    let err = SnapshotsApplierConfig::for_tests()
        .run(&pool, &client, &object_store)
        .await
        .unwrap_err();
    assert!(err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<ObjectStoreError>(),
            Some(ObjectStoreError::KeyNotFound(_))
        )
    }));
}

#[tokio::test]
async fn applier_returns_error_after_too_many_object_store_retries() {
    let pool = ConnectionPool::test_pool().await;
    let expected_status = mock_recovery_status();
    let (object_store, client, _) = prepare_clients(&expected_status).await;
    let object_store = ObjectStoreWithErrors::new(object_store, |_| {
        Err(ObjectStoreError::Other("service not available".into()))
    });

    let err = SnapshotsApplierConfig::for_tests()
        .run(&pool, &client, &object_store)
        .await
        .unwrap_err();
    assert!(err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<ObjectStoreError>(),
            Some(ObjectStoreError::Other(_))
        )
    }));
}
