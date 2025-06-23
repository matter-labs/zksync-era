//! Snapshot applier tests.

use std::{
    future,
    sync::atomic::{AtomicUsize, Ordering},
};

use assert_matches::assert_matches;
use test_casing::test_casing;
use tokio::sync::Barrier;
use zksync_health_check::CheckHealth;
use zksync_object_store::MockObjectStore;
use zksync_types::{
    api::{BlockDetails, L1BatchDetails},
    block::L1BatchHeader,
    get_code_key, L1BatchNumber, ProtocolVersion, ProtocolVersionId,
};

use self::utils::{
    mock_l2_block_header, mock_recovery_status, mock_snapshot_header, mock_tokens, prepare_clients,
    random_storage_logs, MockMainNodeClient, ObjectStoreWithErrors,
};
use super::*;
use crate::tests::utils::{mock_factory_deps, HangingObjectStore};

mod utils;

async fn is_recovery_completed(
    pool: &ConnectionPool<Core>,
    client: &MockMainNodeClient,
) -> RecoveryCompletionStatus {
    let mut connection = pool.connection().await.unwrap();
    SnapshotsApplierTask::is_recovery_completed(&mut connection, client)
        .await
        .unwrap()
}

#[test_casing(3, [(None, false), (Some(2), false), (None, true)])]
#[tokio::test]
async fn snapshots_creator_can_successfully_recover_db(
    pool_size: Option<u32>,
    with_object_store_errors: bool,
) {
    let pool = if let Some(pool_size) = pool_size {
        ConnectionPool::<Core>::constrained_test_pool(pool_size).await
    } else {
        ConnectionPool::<Core>::test_pool().await
    };

    let expected_status = mock_recovery_status();
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;
    let storage_logs_by_hashed_key: HashMap<_, _> =
        storage_logs.into_iter().map(|log| (log.key, log)).collect();

    let object_store_with_errors;
    let object_store = if with_object_store_errors {
        let error_counter = AtomicUsize::new(0);
        object_store_with_errors = ObjectStoreWithErrors::new(object_store, move |_| {
            if error_counter.fetch_add(1, Ordering::SeqCst) >= 3 {
                Ok(()) // "recover" after 3 retries
            } else {
                Err(ObjectStoreError::Other {
                    is_retriable: true,
                    source: "transient error".into(),
                })
            }
        });
        Arc::new(object_store_with_errors)
    } else {
        object_store
    };

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::NoRecoveryDetected,
        "No snapshot information in the DB"
    );

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client.clone()),
        object_store.clone(),
    );
    let task_health = task.health_check();
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let stats = task.run(stop_receiver).await.unwrap();
    assert!(stats.done_work);
    assert_matches!(
        task_health.check_health().await.status(),
        HealthStatus::Ready
    );

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::Completed,
        "Recovery has been completed"
    );

    let mut storage = pool.connection().await.unwrap();
    let mut recovery_dal = storage.snapshot_recovery_dal();

    let current_db_status = recovery_dal.get_applied_snapshot_status().await.unwrap();
    assert_eq!(current_db_status.unwrap(), expected_status);

    let all_initial_writes = storage
        .storage_logs_dedup_dal()
        .dump_all_initial_writes_for_tests()
        .await;
    assert_eq!(all_initial_writes.len(), storage_logs_by_hashed_key.len());
    for initial_write in all_initial_writes {
        let log = &storage_logs_by_hashed_key[&initial_write.hashed_key];
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
    assert_eq!(all_storage_logs.len(), storage_logs_by_hashed_key.len());
    for db_log in all_storage_logs {
        let expected_log = &storage_logs_by_hashed_key[&db_log.hashed_key];
        assert_eq!(db_log.hashed_key, expected_log.key);
        assert!(db_log.key.is_none());
        assert!(db_log.address.is_none());
        assert_eq!(db_log.value, expected_log.value);
        assert_eq!(db_log.l2_block_number, expected_status.l2_block_number);
    }

    // Try recovering again.
    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client.clone()),
        object_store.clone(),
    );

    let (_stop_sender, stop_receiver) = watch::channel(false);
    task.run(stop_receiver).await.unwrap();
    // Here, stats would unfortunately have `done_work: true` because work detection isn't smart enough.

    // Emulate a node processing data after recovery.
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    let l2_block = mock_l2_block_header(expected_status.l2_block_number + 1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();
    drop(storage);

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client),
        object_store,
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let stats = task.run(stop_receiver).await.unwrap();
    assert!(!stats.done_work);
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn applier_recovers_v0_snapshot(drop_storage_key_preimages: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs::<StorageKey>(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;

    let mut task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client),
        object_store,
    );
    if drop_storage_key_preimages {
        task.drop_storage_key_preimages();
    }
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let stats = task.run(stop_receiver).await.unwrap();
    assert!(stats.done_work);

    let mut storage = pool.connection().await.unwrap();
    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert_eq!(all_storage_logs.len(), storage_logs.len());

    let storage_logs_by_hashed_key: HashMap<_, _> = storage_logs
        .into_iter()
        .map(|log| (log.key.hashed_key(), log))
        .collect();
    for db_log in all_storage_logs {
        let expected_log = &storage_logs_by_hashed_key[&db_log.hashed_key];
        assert_eq!(db_log.hashed_key, expected_log.key.hashed_key());
        assert_eq!(db_log.value, expected_log.value);
        assert_eq!(db_log.l2_block_number, expected_status.l2_block_number);

        if drop_storage_key_preimages {
            assert!(db_log.key.is_none());
            assert!(db_log.address.is_none());
        } else {
            assert_eq!(db_log.key, Some(*expected_log.key.key()));
            assert_eq!(db_log.address, Some(*expected_log.key.address()));
        }
    }
}

/// Also tests factory dependencies with the specified bytecode hash.
#[test_casing(3, [None, Some(BytecodeMarker::EraVm), Some(BytecodeMarker::Evm)])]
#[tokio::test]
async fn applier_recovers_explicitly_specified_snapshot(bytecode_kind: Option<BytecodeMarker>) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let factory_deps = mock_factory_deps(bytecode_kind);
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;

    let mut task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client),
        object_store,
    );
    task.set_snapshot_l1_batch(expected_status.l1_batch_number);
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let stats = task.run(stop_receiver).await.unwrap();
    assert!(stats.done_work);

    let mut storage = pool.connection().await.unwrap();
    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert_eq!(all_storage_logs.len(), storage_logs.len());
}

#[tokio::test]
async fn applier_errors_on_unexpected_bytecode_hash() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let mut factory_deps = mock_factory_deps(None);
    factory_deps.factory_deps[0].hash = Some(H256::repeat_byte(0x42)); // unknown bytecode marker
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client),
        object_store,
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let err = task.run(stop_receiver).await.unwrap_err();
    assert!(
        format!("{err:#}").contains("bytecode hash marker"),
        "{err:#}"
    );

    factory_deps.factory_deps[0].hash = Some(BytecodeHash::for_bytecode(&[42; 32]).value());
    let (object_store, client) =
        prepare_clients::<H256>(&expected_status, &factory_deps, &storage_logs).await;

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let err = task.run(stop_receiver).await.unwrap_err();
    assert!(
        format!("{err:#}").contains("restored bytecode hash"),
        "{err:#}"
    );
}

#[tokio::test]
async fn applier_error_for_missing_explicitly_specified_snapshot() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;

    let mut task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    task.set_snapshot_l1_batch(expected_status.l1_batch_number + 1);

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let err = task.run(stop_receiver).await.unwrap_err();
    assert!(
        format!("{err:#}").contains("not present on main node"),
        "{err:#}"
    );
}

#[tokio::test]
async fn snapshot_applier_recovers_after_stopping() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut expected_status = mock_recovery_status();
    expected_status.storage_logs_chunks_processed = vec![true; 10];
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;
    let (stopping_object_store, mut stop_receiver) =
        HangingObjectStore::new(object_store.clone(), 1);

    let mut config = SnapshotsApplierConfig::for_tests();
    config.max_concurrency = NonZeroUsize::new(1).unwrap();
    let task = SnapshotsApplierTask::new(
        config.clone(),
        pool.clone(),
        Box::new(client.clone()),
        Arc::new(stopping_object_store),
    );
    let health_check = task.health_check();
    let (_stop_sender, task_stop_receiver) = watch::channel(false);
    let task_handle = tokio::spawn(task.run(task_stop_receiver));

    // Wait until the first storage logs chunk is requested (the object store hangs up at this point)
    stop_receiver.wait_for(|&count| count > 1).await.unwrap();
    assert!(!task_handle.is_finished());
    let health = health_check.check_health().await;
    let health_details = health.details().unwrap();
    assert_eq!(health_details["factory_deps_recovered"], true);
    assert_eq!(health_details["tokens_recovered"], false);
    assert_eq!(health_details["storage_logs_chunk_count"], 10);
    assert_eq!(health_details["storage_logs_chunks_left_to_process"], 10);
    task_handle.abort();

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::InProgress,
        "Recovery has been aborted"
    );

    // Check that factory deps have been persisted, but no storage logs.
    let mut storage = pool.connection().await.unwrap();
    let all_factory_deps = storage
        .factory_deps_dal()
        .dump_all_factory_deps_for_tests()
        .await;
    assert!(!all_factory_deps.is_empty());
    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert!(all_storage_logs.is_empty(), "{all_storage_logs:?}");

    // Recover 3 storage log chunks and stop again
    let (stopping_object_store, mut stop_receiver) =
        HangingObjectStore::new(object_store.clone(), 3);

    let task = SnapshotsApplierTask::new(
        config.clone(),
        pool.clone(),
        Box::new(client.clone()),
        Arc::new(stopping_object_store),
    );
    let health_check = task.health_check();
    let (_stop_sender, task_stop_receiver) = watch::channel(false);
    let task_handle = tokio::spawn(task.run(task_stop_receiver));

    stop_receiver.wait_for(|&count| count > 3).await.unwrap();
    assert!(!task_handle.is_finished());
    let health = health_check.check_health().await;
    let health_details = health.details().unwrap();
    assert_eq!(health_details["factory_deps_recovered"], true);
    assert_eq!(health_details["tokens_recovered"], false);
    assert_eq!(health_details["storage_logs_chunk_count"], 10);
    assert_eq!(health_details["storage_logs_chunks_left_to_process"], 7);
    task_handle.abort();

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::InProgress,
        "Not all logs have been recovered"
    );

    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert!(all_storage_logs.len() < storage_logs.len());

    // Recover remaining 7 (10 - 3) storage log chunks.
    let (stopping_object_store, _) = HangingObjectStore::new(object_store.clone(), 7);
    let mut task = SnapshotsApplierTask::new(
        config,
        pool.clone(),
        Box::new(client.clone()),
        Arc::new(stopping_object_store),
    );
    task.set_snapshot_l1_batch(expected_status.l1_batch_number); // check that this works fine
    let (_stop_sender, stop_receiver) = watch::channel(false);
    task.run(stop_receiver).await.unwrap();

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::Completed,
        "Recovery has been completed"
    );

    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert_eq!(all_storage_logs.len(), storage_logs.len());
}

#[tokio::test]
async fn health_status_immediately_after_task_start() {
    #[derive(Debug, Clone)]
    struct HangingMainNodeClient(Arc<Barrier>);

    #[async_trait]
    impl SnapshotsApplierMainNodeClient for HangingMainNodeClient {
        async fn fetch_l1_batch_details(
            &self,
            _number: L1BatchNumber,
        ) -> EnrichedClientResult<Option<L1BatchDetails>> {
            self.0.wait().await;
            future::pending().await
        }

        async fn fetch_l2_block_details(
            &self,
            _number: L2BlockNumber,
        ) -> EnrichedClientResult<Option<BlockDetails>> {
            self.0.wait().await;
            future::pending().await
        }

        async fn fetch_newest_snapshot_l1_batch_number(
            &self,
        ) -> EnrichedClientResult<Option<L1BatchNumber>> {
            self.0.wait().await;
            future::pending().await
        }

        async fn fetch_snapshot(
            &self,
            _l1_batch_number: L1BatchNumber,
        ) -> EnrichedClientResult<Option<SnapshotHeader>> {
            self.0.wait().await;
            future::pending().await
        }

        async fn fetch_tokens(
            &self,
            _at_l2_block: L2BlockNumber,
        ) -> EnrichedClientResult<Vec<TokenInfo>> {
            self.0.wait().await;
            future::pending().await
        }
    }

    let object_store = MockObjectStore::arc();
    let client = HangingMainNodeClient(Arc::new(Barrier::new(2)));
    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        ConnectionPool::<Core>::test_pool().await,
        Box::new(client.clone()),
        object_store,
    );
    let task_health = task.health_check();
    let (_stop_sender, task_stop_receiver) = watch::channel(false);
    let task_handle = tokio::spawn(task.run(task_stop_receiver));

    client.0.wait().await; // Wait for the first L2 client call (at which point, the task is certainly initialized)
    assert_matches!(
        task_health.check_health().await.status(),
        HealthStatus::Affected
    );
    task_handle.abort();
}

#[tokio::test]
async fn applier_errors_after_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    // We don't want to depend on the core crate, so instead we cheaply emulate it.
    let mut storage = pool.connection().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    let genesis_l2_block = mock_l2_block_header(L2BlockNumber(0));
    storage
        .blocks_dal()
        .insert_l2_block(&genesis_l2_block)
        .await
        .unwrap();
    let genesis_l1_batch = L1BatchHeader::mock(
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
        .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await
        .unwrap();
    drop(storage);

    let object_store = MockObjectStore::arc();
    let client = MockMainNodeClient::default();

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    let (_stop_sender, task_stop_receiver) = watch::channel(false);
    task.run(task_stop_receiver).await.unwrap_err();
}

#[tokio::test]
async fn applier_errors_without_snapshots() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let object_store = MockObjectStore::arc();
    let client = MockMainNodeClient::default();

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    task.run(stop_receiver).await.unwrap_err();
}

#[tokio::test]
async fn applier_errors_with_unrecognized_snapshot_version() {
    let pool = ConnectionPool::test_pool().await;
    let object_store = MockObjectStore::arc();
    let expected_status = mock_recovery_status();
    let client = MockMainNodeClient {
        fetch_newest_snapshot_response: Some(mock_snapshot_header(u16::MAX, &expected_status)),
        ..MockMainNodeClient::default()
    };

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    task.run(stop_receiver).await.unwrap_err();
}

#[tokio::test]
async fn applier_returns_error_on_fatal_object_store_error() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 100);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;
    let object_store = ObjectStoreWithErrors::new(object_store, |_| {
        Err(ObjectStoreError::KeyNotFound("not found".into()))
    });

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        Arc::new(object_store),
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let OrStopped::Internal(err) = task.run(stop_receiver).await.unwrap_err() else {
        panic!("Recovery unexpectedly stopped");
    };
    assert!(err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<ObjectStoreError>(),
            Some(ObjectStoreError::KeyNotFound(_))
        )
    }));
}

#[tokio::test]
async fn applier_returns_error_after_too_many_object_store_retries() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 100);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;
    let object_store = ObjectStoreWithErrors::new(object_store, |_| {
        Err(ObjectStoreError::Other {
            is_retriable: true,
            source: "service not available".into(),
        })
    });

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        Arc::new(object_store),
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let OrStopped::Internal(err) = task.run(stop_receiver).await.unwrap_err() else {
        panic!("Recovery unexpectedly stopped");
    };

    assert!(err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<ObjectStoreError>(),
            Some(ObjectStoreError::Other { .. })
        )
    }));
}

#[tokio::test]
async fn recovering_tokens() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let tokens = mock_tokens();
    let factory_deps = mock_factory_deps(None);
    let mut storage_logs = random_storage_logs(expected_status.l1_batch_number, 200);
    for token in &tokens {
        if token.l2_address.is_zero() {
            continue;
        }
        storage_logs.push(SnapshotStorageLog {
            key: get_code_key(&token.l2_address).hashed_key(),
            value: H256::random(),
            l1_batch_number_of_initial_write: expected_status.l1_batch_number,
            enumeration_index: storage_logs.len() as u64 + 1,
        });
    }
    let (object_store, mut client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;

    client.tokens_response.clone_from(&tokens);

    // Make sure that the task will fail when we will start migrating tokens.
    client.set_token_response_error(EnrichedClientError::custom("Error", "not_important"));

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client.clone()),
        object_store.clone(),
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let task_result = task.run(stop_receiver).await;
    assert!(task_result.is_err());

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::InProgress,
        "Tokens are not migrated"
    );

    // Now perform the recovery again, tokens should be migrated.
    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client.clone()),
        object_store.clone(),
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    task.run(stop_receiver).await.unwrap();

    assert_eq!(
        is_recovery_completed(&pool, &client).await,
        RecoveryCompletionStatus::Completed,
        "Recovery is completed"
    );

    // Check that tokens are successfully restored.
    let mut storage = pool.connection().await.unwrap();
    let recovered_tokens = storage
        .tokens_web3_dal()
        .get_all_tokens(None)
        .await
        .unwrap();
    // Since we cannot guarantee token ordering, we need to convert them to maps.
    let token_map: HashMap<_, _> = tokens
        .into_iter()
        .map(|token| (token.l2_address, token))
        .collect();
    let recovered_token_map: HashMap<_, _> = recovered_tokens
        .into_iter()
        .map(|token| (token.l2_address, token))
        .collect();
    assert_eq!(token_map, recovered_token_map);
    drop(storage);

    // Check that recovering again works and is a no-op.
    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    let (_stop_sender, stop_receiver) = watch::channel(false);
    task.run(stop_receiver).await.unwrap();
}

#[tokio::test]
async fn snapshot_applier_can_be_canceled() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut expected_status = mock_recovery_status();
    expected_status.storage_logs_chunks_processed = vec![true; 10];
    let factory_deps = mock_factory_deps(None);
    let storage_logs = random_storage_logs::<H256>(expected_status.l1_batch_number, 200);
    let (object_store, client) =
        prepare_clients(&expected_status, &factory_deps, &storage_logs).await;
    let (stopping_object_store, mut stop_receiver) =
        HangingObjectStore::new(object_store.clone(), 1);

    let mut config = SnapshotsApplierConfig::for_tests();
    config.max_concurrency = NonZeroUsize::new(1).unwrap();
    let task = SnapshotsApplierTask::new(
        config.clone(),
        pool.clone(),
        Box::new(client.clone()),
        Arc::new(stopping_object_store),
    );
    let (task_stop_sender, task_stop_receiver) = watch::channel(false);
    let task_handle = tokio::spawn(task.run(task_stop_receiver));

    // Wait until the first storage logs chunk is requested (the object store hangs up at this point)
    stop_receiver.wait_for(|&count| count > 1).await.unwrap();
    assert!(!task_handle.is_finished());

    task_stop_sender.send(true).unwrap();
    let err = tokio::time::timeout(Duration::from_secs(5), task_handle)
        .await
        .expect("Task wasn't canceled")
        .unwrap()
        .unwrap_err();
    assert_matches!(err, OrStopped::Stopped);
}
