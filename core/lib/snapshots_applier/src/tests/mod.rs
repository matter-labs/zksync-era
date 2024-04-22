//! Snapshot applier tests.

use std::{
    future,
    sync::atomic::{AtomicUsize, Ordering},
};

use assert_matches::assert_matches;
use test_casing::test_casing;
use tokio::sync::Barrier;
use zksync_health_check::CheckHealth;
use zksync_object_store::ObjectStoreFactory;
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

mod utils;

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
    let storage_logs = random_storage_logs(expected_status.l1_batch_number, 200);
    let (object_store, client) = prepare_clients(&expected_status, &storage_logs).await;
    let storage_logs_by_hashed_key: HashMap<_, _> = storage_logs
        .into_iter()
        .map(|log| (log.key.hashed_key(), log))
        .collect();

    let object_store_with_errors;
    let object_store = if with_object_store_errors {
        let error_counter = AtomicUsize::new(0);
        object_store_with_errors = ObjectStoreWithErrors::new(object_store, move |_| {
            if error_counter.fetch_add(1, Ordering::SeqCst) >= 3 {
                Ok(()) // "recover" after 3 retries
            } else {
                Err(ObjectStoreError::Other("transient error".into()))
            }
        });
        Arc::new(object_store_with_errors)
    } else {
        object_store
    };

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client.clone()),
        object_store.clone(),
    );
    let task_health = task.health_check();
    let stats = task.run().await.unwrap();
    assert!(stats.done_work);
    assert_matches!(
        task_health.check_health().await.status(),
        HealthStatus::Ready
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
        assert_eq!(db_log.address, *expected_log.key.address());
        assert_eq!(db_log.key, *expected_log.key.key());
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
    task.run().await.unwrap();
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
    let stats = task.run().await.unwrap();
    assert!(!stats.done_work);
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

        async fn fetch_newest_snapshot(&self) -> EnrichedClientResult<Option<SnapshotHeader>> {
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

    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = HangingMainNodeClient(Arc::new(Barrier::new(2)));
    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        ConnectionPool::<Core>::test_pool().await,
        Box::new(client.clone()),
        object_store,
    );
    let task_health = task.health_check();
    let task_handle = tokio::spawn(task.run());

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
        .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await
        .unwrap();
    drop(storage);

    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = MockMainNodeClient::default();

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    task.run().await.unwrap_err();
}

#[tokio::test]
async fn applier_errors_without_snapshots() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = MockMainNodeClient::default();

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    task.run().await.unwrap_err();
}

#[tokio::test]
async fn applier_errors_with_unrecognized_snapshot_version() {
    let pool = ConnectionPool::test_pool().await;
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let expected_status = mock_recovery_status();
    let client = MockMainNodeClient {
        fetch_newest_snapshot_response: Some(SnapshotHeader {
            version: u16::MAX,
            ..mock_snapshot_header(&expected_status)
        }),
        ..MockMainNodeClient::default()
    };

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        object_store,
    );
    task.run().await.unwrap_err();
}

#[tokio::test]
async fn applier_returns_error_on_fatal_object_store_error() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let storage_logs = random_storage_logs(expected_status.l1_batch_number, 100);
    let (object_store, client) = prepare_clients(&expected_status, &storage_logs).await;
    let object_store = ObjectStoreWithErrors::new(object_store, |_| {
        Err(ObjectStoreError::KeyNotFound("not found".into()))
    });

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        Arc::new(object_store),
    );
    let err = task.run().await.unwrap_err();
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
    let storage_logs = random_storage_logs(expected_status.l1_batch_number, 100);
    let (object_store, client) = prepare_clients(&expected_status, &storage_logs).await;
    let object_store = ObjectStoreWithErrors::new(object_store, |_| {
        Err(ObjectStoreError::Other("service not available".into()))
    });

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool,
        Box::new(client),
        Arc::new(object_store),
    );
    let err = task.run().await.unwrap_err();
    assert!(err.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<ObjectStoreError>(),
            Some(ObjectStoreError::Other(_))
        )
    }));
}

#[tokio::test]
async fn recovering_tokens() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let expected_status = mock_recovery_status();
    let tokens = mock_tokens();
    let mut storage_logs = random_storage_logs(expected_status.l1_batch_number, 200);
    for token in &tokens {
        if token.l2_address.is_zero() {
            continue;
        }
        storage_logs.push(SnapshotStorageLog {
            key: get_code_key(&token.l2_address),
            value: H256::random(),
            l1_batch_number_of_initial_write: expected_status.l1_batch_number,
            enumeration_index: storage_logs.len() as u64 + 1,
        });
    }
    let (object_store, mut client) = prepare_clients(&expected_status, &storage_logs).await;
    client.tokens_response = tokens.clone();

    let task = SnapshotsApplierTask::new(
        SnapshotsApplierConfig::for_tests(),
        pool.clone(),
        Box::new(client.clone()),
        object_store.clone(),
    );
    task.run().await.unwrap();

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
    task.run().await.unwrap();
}
