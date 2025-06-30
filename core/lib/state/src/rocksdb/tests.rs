//! Tests for [`RocksdbStorage`].

use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc,
    },
};

use assert_matches::assert_matches;
use async_trait::async_trait;
use tempfile::TempDir;
use test_casing::test_casing;
use zksync_dal::{ConnectionPool, Core};
use zksync_types::{L2BlockNumber, StorageLog};

use super::*;
use crate::test_utils::{
    create_l1_batch, create_l2_block, gen_storage_logs, mock_snapshot_recovery_status,
    prepare_postgres, prepare_postgres_for_snapshot_recovery, prepare_postgres_with_log_count,
    prune_storage,
};

#[async_trait]
pub(super) trait AsyncHandler<Arg>: 'static + Send + Sync {
    async fn handle(&self, arg: Arg);
}

#[async_trait]
impl<T: Send + 'static> AsyncHandler<T> for () {
    async fn handle(&self, _arg: T) {
        // Do nothing
    }
}

#[async_trait]
impl<T: Send + 'static, F: Fn(T) + Send + Sync + 'static> AsyncHandler<T> for F {
    async fn handle(&self, arg: T) {
        self(arg);
    }
}

#[derive(Clone)]
pub(super) struct RocksdbStorageEventListener {
    /// Called when an L1 batch is synced.
    pub on_l1_batch_synced: Arc<dyn AsyncHandler<L1BatchNumber>>,
    /// Called when a storage logs chunk is recovered from a snapshot.
    pub on_logs_chunk_recovered: Arc<dyn AsyncHandler<u64>>,
}

impl fmt::Debug for RocksdbStorageEventListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RocksdbStorageEventListener")
            .finish_non_exhaustive()
    }
}

impl Default for RocksdbStorageEventListener {
    fn default() -> Self {
        Self {
            on_l1_batch_synced: Arc::new(()),
            on_logs_chunk_recovered: Arc::new(()),
        }
    }
}

fn hash_storage_log_keys(logs: &HashMap<StorageKey, H256>) -> HashMap<H256, H256> {
    logs.iter()
        .map(|(key, value)| (key.hashed_key(), *value))
        .collect()
}

#[tokio::test]
async fn rocksdb_storage_basics() {
    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let mut storage_logs: HashMap<_, _> = gen_storage_logs(0..20)
        .into_iter()
        .map(|log| (log.key, log.value))
        .collect();
    let changed_keys =
        RocksdbStorage::process_transaction_logs(&storage.db, hash_storage_log_keys(&storage_logs));
    storage.pending_patch.state = changed_keys
        .into_iter()
        .map(|(key, state_value)| (key, (state_value.value, 1))) // enum index doesn't matter in the test
        .collect();
    storage.save(Some(L1BatchNumber(0))).await.unwrap();
    {
        for (key, value) in &storage_logs {
            assert!(!storage.is_write_initial(key));
            assert_eq!(storage.read_value(key), *value);
        }
    }

    // Overwrite some of the logs.
    for log_value in storage_logs.values_mut().step_by(2) {
        *log_value = StorageValue::zero();
    }
    let changed_keys =
        RocksdbStorage::process_transaction_logs(&storage.db, hash_storage_log_keys(&storage_logs));
    storage.pending_patch.state = changed_keys
        .into_iter()
        .map(|(key, state_value)| (key, (state_value.value, 1))) // enum index doesn't matter in the test
        .collect();
    storage.save(Some(L1BatchNumber(1))).await.unwrap();

    for (key, value) in &storage_logs {
        assert!(!storage.is_write_initial(key));
        assert_eq!(storage.read_value(key), *value);
    }
}

async fn sync_test_storage(dir: &TempDir, pool: &ConnectionPool<Core>) -> RocksdbStorage {
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let (rocksdb, _) = RocksdbStorage::builder(dir.path())
        .await
        .expect("Failed initializing RocksDB")
        .ensure_ready(pool, &stop_receiver)
        .await
        .expect("Failed recovering RocksDB");
    let mut conn = pool.connection().await.unwrap();
    rocksdb
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap()
}

async fn sync_test_storage_and_check_recovery(
    dir: &TempDir,
    pool: &ConnectionPool<Core>,
    expect_recovery: bool,
) -> RocksdbStorage {
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let (rocksdb, init_strategy) = RocksdbStorage::builder(dir.path())
        .await
        .expect("Failed initializing RocksDB")
        .ensure_ready(pool, &stop_receiver)
        .await
        .unwrap();
    assert_eq!(
        matches!(init_strategy, InitStrategy::Recovery),
        expect_recovery
    );

    let mut conn = pool.connection().await.unwrap();
    rocksdb
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap()
}

#[tokio::test]
async fn rocksdb_storage_syncing_with_postgres() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut conn).await;
    let storage_logs = gen_storage_logs(20..40);
    create_l2_block(&mut conn, L2BlockNumber(1), &storage_logs, L1BatchNumber(1)).await;
    create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;
    drop(conn);

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &pool).await;

    assert_eq!(storage.next_l1_batch_number().await, L1BatchNumber(2));
    for log in &storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
    }
}

#[tokio::test]
async fn rocksdb_storage_syncing_fault_tolerance() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut conn).await;
    let storage_logs = gen_storage_logs(100..200);
    for (i, block_logs) in storage_logs.chunks(20).enumerate() {
        let number = u32::try_from(i).unwrap() + 1;
        create_l2_block(
            &mut conn,
            L2BlockNumber(number),
            block_logs,
            L1BatchNumber(number),
        )
        .await;
        create_l1_batch(&mut conn, L1BatchNumber(number), block_logs).await;
    }

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let (stop_sender, stop_receiver) = watch::channel(false);
    let (mut storage, _) = RocksdbStorage::builder(dir.path())
        .await
        .expect("Failed initializing RocksDB")
        .ensure_ready(&pool, &stop_receiver)
        .await
        .unwrap();
    let expected_l1_batch_number = AtomicU32::new(0);
    storage.listener.on_l1_batch_synced = Arc::new(move |number: L1BatchNumber| {
        assert_eq!(
            number.0,
            expected_l1_batch_number.fetch_add(1, Ordering::Relaxed)
        );
        if number == L1BatchNumber(2) {
            stop_sender.send_replace(true);
        }
    });
    let err = storage
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap_err();
    assert_matches!(err, OrStopped::Stopped);

    // Resume storage syncing and check that it completes.
    let storage = RocksdbStorage::builder(dir.path())
        .await
        .expect("Failed initializing RocksDB");
    assert_eq!(storage.next_l1_batch_number().await, Some(L1BatchNumber(3)));
    let storage = storage.get().await.unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut storage = storage
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap();
    assert_eq!(storage.next_l1_batch_number().await, L1BatchNumber(6));
    for log in &storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
        assert!(!storage.is_write_initial(&log.key));
    }
}

async fn insert_factory_deps(
    conn: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
    indices: impl Iterator<Item = u8>,
) {
    let factory_deps = indices
        .map(|i| (H256::repeat_byte(i), vec![i; 64]))
        .collect();
    conn.factory_deps_dal()
        .insert_factory_deps(l2_block_number, &factory_deps)
        .await
        .unwrap();
}

#[tokio::test]
async fn rocksdb_storage_revert() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut conn).await;
    let storage_logs = gen_storage_logs(20..40);
    create_l2_block(
        &mut conn,
        L2BlockNumber(1),
        &storage_logs[..10],
        L1BatchNumber(1),
    )
    .await;
    insert_factory_deps(&mut conn, L2BlockNumber(1), 0..1).await;
    create_l2_block(
        &mut conn,
        L2BlockNumber(2),
        &storage_logs[10..],
        L1BatchNumber(1),
    )
    .await;
    insert_factory_deps(&mut conn, L2BlockNumber(2), 1..3).await;
    create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

    let inserted_storage_logs = gen_storage_logs(50..60);
    let replaced_storage_logs: Vec<_> = storage_logs
        .iter()
        .step_by(2)
        .map(|&log| StorageLog {
            value: H256::repeat_byte(0xf0),
            ..log
        })
        .collect();

    let mut new_storage_logs = inserted_storage_logs.clone();
    new_storage_logs.extend_from_slice(&replaced_storage_logs);
    create_l2_block(
        &mut conn,
        L2BlockNumber(3),
        &new_storage_logs,
        L1BatchNumber(2),
    )
    .await;
    insert_factory_deps(&mut conn, L2BlockNumber(3), 3..5).await;
    create_l1_batch(&mut conn, L1BatchNumber(2), &inserted_storage_logs).await;

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &pool).await;

    // Perform some sanity checks before the revert.
    assert_eq!(storage.next_l1_batch_number().await, L1BatchNumber(3));
    {
        for log in &inserted_storage_logs {
            assert_eq!(storage.read_value(&log.key), log.value);
        }
        for log in &replaced_storage_logs {
            assert_eq!(storage.read_value(&log.key), log.value);
        }

        for i in 0..5 {
            assert_eq!(
                storage.load_factory_dep(H256::repeat_byte(i)).unwrap(),
                [i; 64]
            );
        }
    }

    storage
        .roll_back(&mut conn, L1BatchNumber(1))
        .await
        .unwrap();
    assert_eq!(storage.next_l1_batch_number().await, L1BatchNumber(2));
    {
        for log in &inserted_storage_logs {
            assert_eq!(storage.read_value(&log.key), H256::zero());
        }
        for log in &replaced_storage_logs {
            assert_ne!(storage.read_value(&log.key), log.value);
        }

        for i in 0..3 {
            assert_eq!(
                storage.load_factory_dep(H256::repeat_byte(i)).unwrap(),
                [i; 64]
            );
        }
        for i in 3..5 {
            assert!(storage.load_factory_dep(H256::repeat_byte(i)).is_none());
        }
    }
}

#[test_casing(4, [RocksdbStorage::DESIRED_LOG_CHUNK_SIZE, 20, 5, 1])]
#[tokio::test]
async fn low_level_snapshot_recovery(log_chunk_size: u64) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let (snapshot_recovery, mut storage_logs) =
        prepare_postgres_for_snapshot_recovery(&mut conn).await;
    drop(conn);

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let (_stop_sender, stop_receiver) = watch::channel(false);
    storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap();
    assert_eq!(
        storage.next_l1_batch_number().await,
        snapshot_recovery.l1_batch_number + 1
    );

    // Sort logs in the same order as enum indices are assigned (by full `StorageKey`).
    storage_logs.sort_unstable_by_key(|log| log.key);
    for (i, log) in storage_logs.iter().enumerate() {
        assert_eq!(storage.read_value(&log.key), log.value);
        let expected_index = i as u64 + 1;
        assert_eq!(
            storage.get_enumeration_index(&log.key),
            Some(expected_index)
        );
    }
}

#[tokio::test]
async fn recovering_factory_deps_from_snapshot() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let (snapshot_recovery, _) = prepare_postgres_for_snapshot_recovery(&mut conn).await;

    let mut all_factory_deps = HashMap::new();
    for number in 0..snapshot_recovery.l2_block_number.0 {
        let bytecode_hash = H256::from_low_u64_be(number.into());
        let bytecode = vec![u8::try_from(number).unwrap(); 1_024];
        all_factory_deps.insert(bytecode_hash, bytecode.clone());

        let number = L2BlockNumber(number);
        conn.factory_deps_dal()
            .insert_factory_deps(number, &HashMap::from([(bytecode_hash, bytecode)]))
            .await
            .unwrap();
    }
    drop(conn);

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &pool).await;

    for (bytecode_hash, bytecode) in &all_factory_deps {
        assert_eq!(storage.load_factory_dep(*bytecode_hash).unwrap(), *bytecode);
    }
}

#[tokio::test]
async fn recovering_from_snapshot_and_following_logs() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let (snapshot_recovery, mut storage_logs) =
        prepare_postgres_for_snapshot_recovery(&mut conn).await;

    // Add some more storage logs.
    let new_storage_logs = gen_storage_logs(500..600);
    create_l2_block(
        &mut conn,
        snapshot_recovery.l2_block_number + 1,
        &new_storage_logs,
        snapshot_recovery.l1_batch_number + 1,
    )
    .await;
    create_l1_batch(
        &mut conn,
        snapshot_recovery.l1_batch_number + 1,
        &new_storage_logs,
    )
    .await;

    let updated_storage_logs: Vec<_> = storage_logs
        .iter()
        .step_by(3)
        .copied()
        .map(|mut log| {
            log.value = H256::repeat_byte(0xff);
            log
        })
        .collect();
    create_l2_block(
        &mut conn,
        snapshot_recovery.l2_block_number + 2,
        &updated_storage_logs,
        snapshot_recovery.l1_batch_number + 2,
    )
    .await;
    create_l1_batch(&mut conn, snapshot_recovery.l1_batch_number + 2, &[]).await;
    drop(conn);

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage_and_check_recovery(&dir, &pool, true).await;

    for (i, log) in new_storage_logs.iter().enumerate() {
        assert_eq!(storage.read_value(&log.key), log.value);
        let expected_index = (i + storage_logs.len()) as u64 + 1;
        assert_eq!(
            storage.get_enumeration_index(&log.key),
            Some(expected_index)
        );
        assert!(!storage.is_write_initial(&log.key));
    }

    for log in &updated_storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
        assert!(storage.get_enumeration_index(&log.key).unwrap() <= storage_logs.len() as u64);
    }
    storage_logs.sort_unstable_by_key(|log| log.key);
    for (i, log) in storage_logs.iter().enumerate() {
        let expected_index = i as u64 + 1;
        assert_eq!(
            storage.get_enumeration_index(&log.key),
            Some(expected_index)
        );
        assert!(!storage.is_write_initial(&log.key));
    }

    drop(storage);
    sync_test_storage_and_check_recovery(&dir, &pool, false).await;
}

async fn partially_recover_storage(
    pool: &ConnectionPool<Core>,
    temp_dir: &TempDir,
    from_genesis: bool,
) -> (Vec<StorageLog>, u64) {
    assert_eq!(pool.max_size(), 1);

    let mut conn = pool.connection().await.unwrap();
    let storage_logs = if from_genesis {
        prepare_postgres_with_log_count(&mut conn, 2_000).await
    } else {
        prepare_postgres_for_snapshot_recovery(&mut conn).await.1
    };
    drop(conn);
    let log_chunk_size = storage_logs.len() as u64 / 5;

    let mut storage = RocksdbStorage::new(temp_dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let synced_chunk_count = AtomicU64::new(0);
    storage.listener.on_logs_chunk_recovered = Arc::new(move |_| {
        let synced_chunk_count = synced_chunk_count.fetch_add(1, Ordering::Relaxed) + 1;
        if synced_chunk_count == 3 {
            stop_sender.send_replace(true);
        }
    });

    let err = storage
        .ensure_ready(pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap_err();
    assert_matches!(err, OrStopped::Stopped);

    let recovery_l1_batch = storage.recovery_l1_batch_number().await.unwrap();
    let recovery_l1_batch = recovery_l1_batch.expect("no recovery batch");
    if from_genesis {
        assert_eq!(recovery_l1_batch, L1BatchNumber(0));
    } else {
        assert!(recovery_l1_batch > L1BatchNumber(0));
    }

    (storage_logs, log_chunk_size)
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn recovery_fault_tolerance(from_genesis: bool) {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");

    let (storage_logs, log_chunk_size) = partially_recover_storage(&pool, &dir, from_genesis).await;

    // Resume recovery and check that no chunks are recovered twice.
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let synced_chunk_count = Arc::new(AtomicU64::new(0));
    let synced_chunk_count_for_listener = synced_chunk_count.clone();
    storage.listener.on_logs_chunk_recovered = Arc::new(move |_| {
        synced_chunk_count_for_listener.fetch_add(1, Ordering::Relaxed);
    });

    let strategy = storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap();
    assert_matches!(strategy, InitStrategy::Recovery);

    let next_l1_batch = storage.next_l1_batch_number().await;
    if from_genesis {
        assert_eq!(next_l1_batch, L1BatchNumber(1));
    } else {
        assert!(next_l1_batch > L1BatchNumber(1));
    }
    let recovery_l1_batch = storage.recovery_l1_batch_number().await.unwrap();
    assert_eq!(recovery_l1_batch, None); // unset at the end of recovery

    let expected_synced_chunk_count = (storage_logs.len() as u64).div_ceil(log_chunk_size) - 3;
    assert_eq!(
        synced_chunk_count.load(Ordering::Relaxed),
        expected_synced_chunk_count
    );

    for log in &storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
        assert!(!storage.is_write_initial(&log.key));
    }
}

#[tokio::test]
async fn entire_workflow_for_genesis_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut conn).await;
    // Add more genesis storage logs so that the recovery strategy is triggered.
    let more_logs = gen_storage_logs(100..2_100);
    conn.storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &more_logs)
        .await
        .unwrap();
    drop(conn);

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();

    let log_chunk_size = 100;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let strategy = storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap();
    assert_matches!(strategy, InitStrategy::Recovery);
    let next_l1_batch = storage.next_l1_batch_number().await;
    assert_eq!(next_l1_batch, L1BatchNumber(1));

    let recovery_l1_batch = storage.recovery_l1_batch_number().await.unwrap();
    assert_eq!(recovery_l1_batch, None); // should be unset at the end of recovery
}

#[tokio::test]
async fn recovery_with_pruning() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut conn).await;

    let mut storage_logs = gen_storage_logs(20..40);
    create_l2_block(&mut conn, L2BlockNumber(1), &storage_logs, L1BatchNumber(1)).await;
    create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

    // Override a part of storage logs.
    let mut overwritten_logs = storage_logs.split_off(10);
    for log in &mut overwritten_logs {
        log.value = H256::repeat_byte(0xff);
    }
    create_l2_block(
        &mut conn,
        L2BlockNumber(2),
        &overwritten_logs,
        L1BatchNumber(2),
    )
    .await;
    create_l1_batch(&mut conn, L1BatchNumber(2), &[]).await;

    prune_storage(&pool, L1BatchNumber(1)).await;

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();

    let log_chunk_size = 10;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let strategy = storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap();
    assert_matches!(strategy, InitStrategy::Recovery);
    let next_l1_batch = storage.next_l1_batch_number().await;
    assert_eq!(next_l1_batch, L1BatchNumber(2));

    for log in &storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
    }
    for log in &overwritten_logs {
        assert_ne!(storage.read_value(&log.key), log.value);
    }

    // Synchronize the storage and check overwritten logs.
    let is_synced = Arc::new(AtomicBool::default());
    let is_synced_for_listener = is_synced.clone();
    storage.listener.on_l1_batch_synced = Arc::new(move |number| {
        assert_eq!(number, L1BatchNumber(2));
        is_synced_for_listener.store(true, Ordering::Relaxed);
    });
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut storage = storage
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap();

    assert!(is_synced.load(Ordering::Relaxed));
    for log in storage_logs.iter().chain(&overwritten_logs) {
        assert_eq!(storage.read_value(&log.key), log.value);
    }
}

#[tokio::test]
async fn recovery_with_pruning_and_overwritten_logs() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut conn).await;

    let mut storage_logs = gen_storage_logs(20..40);
    create_l2_block(&mut conn, L2BlockNumber(1), &storage_logs, L1BatchNumber(1)).await;
    create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

    for log in &mut storage_logs {
        log.value = H256::repeat_byte(0xff);
    }
    create_l2_block(&mut conn, L2BlockNumber(2), &storage_logs, L1BatchNumber(2)).await;
    create_l1_batch(&mut conn, L1BatchNumber(2), &[]).await;

    // Create another batch to not end up with an empty storage after pruning
    let new_logs = gen_storage_logs(40..60);
    create_l2_block(&mut conn, L2BlockNumber(3), &new_logs, L1BatchNumber(3)).await;
    create_l1_batch(&mut conn, L1BatchNumber(3), &new_logs).await;
    drop(conn);

    let pruning_stats = prune_storage(&pool, L1BatchNumber(2)).await;
    assert!(pruning_stats.deleted_storage_logs > 0);

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let log_chunk_size = 10;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let strategy = storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap();
    assert_matches!(strategy, InitStrategy::Recovery);
    let next_l1_batch = storage.next_l1_batch_number().await;
    assert_eq!(next_l1_batch, L1BatchNumber(3));

    for log in &storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn recovery_detects_additional_pruning_after_restart(from_genesis: bool) {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");

    let (_, log_chunk_size) = partially_recover_storage(&pool, &dir, from_genesis).await;

    // Create 2 batches and prune one of them from the storage. This is required to always prune some data.
    let (latest_l1_batch, latest_l2_block) = if from_genesis {
        (L1BatchNumber(0), L2BlockNumber(0))
    } else {
        let status = mock_snapshot_recovery_status();
        (status.l1_batch_number, status.l2_block_number)
    };

    let mut conn = pool.connection().await.unwrap();
    for i in 1_u32..=2 {
        let log_start_idx = 10_000 * u64::from(i);
        let new_logs = gen_storage_logs(log_start_idx..log_start_idx + 50);
        create_l2_block(
            &mut conn,
            latest_l2_block + i,
            &new_logs,
            latest_l1_batch + i,
        )
        .await;
        create_l1_batch(&mut conn, latest_l1_batch + i, &new_logs).await;
    }
    drop(conn);
    prune_storage(&pool, latest_l1_batch + 1).await;

    // Restart recovery. Additional pruning should be detected.
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let err = storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap_err();
    let OrStopped::Internal(err) = err else {
        panic!("Expected internal error");
    };
    assert!(
        format!("{err:#}").contains("Snapshot parameters in Postgres"),
        "{err:#}"
    );
}

/// Prunes the storage while recovery is in progress.
#[derive(Debug)]
struct PruningHandler {
    pool: ConnectionPool<Core>,
    synced_chunk_count: AtomicU64,
}

#[async_trait]
impl AsyncHandler<u64> for PruningHandler {
    async fn handle(&self, _chunk_id: u64) {
        let synced_chunk_count = self.synced_chunk_count.fetch_add(1, Ordering::SeqCst) + 1;
        if synced_chunk_count == 3 {
            prune_storage(&self.pool, L1BatchNumber(1)).await;
        }
    }
}

#[tokio::test]
async fn recovery_detects_additional_pruning_in_progress() {
    // We need a singleton pool to guarantee that all pruning checks won't run before pruning is triggered
    // by `PruningHandler`.
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut conn = pool.connection().await.unwrap();
    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");

    prepare_postgres_with_log_count(&mut conn, 2_000).await;
    for i in 1_u32..=2 {
        let log_start_idx = 10_000 * u64::from(i);
        let new_logs = gen_storage_logs(log_start_idx..log_start_idx + 50);
        create_l2_block(&mut conn, L2BlockNumber(i), &new_logs, L1BatchNumber(i)).await;
        create_l1_batch(&mut conn, L1BatchNumber(i), &new_logs).await;
    }
    drop(conn);

    let mut storage = RocksdbStorage::new(dir.path().into(), RocksdbStorageOptions::default())
        .await
        .unwrap();
    let (_stop_sender, stop_receiver) = watch::channel(false);
    storage.listener.on_logs_chunk_recovered = Arc::new(PruningHandler {
        pool: pool.clone(),
        synced_chunk_count: AtomicU64::new(0),
    });

    let log_chunk_size = 10;
    let err = storage
        .ensure_ready(&pool, log_chunk_size, &stop_receiver)
        .await
        .unwrap_err();
    let OrStopped::Internal(err) = err else {
        panic!("Expected internal error");
    };
    assert!(
        format!("{err:#}").contains("recovery is impossible"),
        "{err:#}"
    );
}
