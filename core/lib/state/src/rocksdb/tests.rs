//! Tests for [`RocksdbStorage`].

use tempfile::TempDir;
use zksync_dal::ConnectionPool;
use zksync_types::{MiniblockNumber, StorageLog};

use super::*;
use crate::test_utils::{
    create_l1_batch, create_miniblock, gen_storage_logs, prepare_postgres,
    prepare_postgres_for_snapshot_recovery,
};

#[tokio::test]
async fn rocksdb_storage_basics() {
    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().to_path_buf()).await.unwrap();
    let mut storage_logs: HashMap<_, _> = gen_storage_logs(0..20)
        .into_iter()
        .map(|log| (log.key, log.value))
        .collect();
    let changed_keys = RocksdbStorage::process_transaction_logs(&storage.db, storage_logs.clone());
    storage.pending_patch.state = changed_keys
        .into_iter()
        .map(|(key, state_value)| (key.hashed_key(), (state_value.value, 1))) // enum index doesn't matter in the test
        .collect();
    storage.save(Some(L1BatchNumber(0))).await.unwrap();
    {
        for (key, value) in &storage_logs {
            assert!(!storage.is_write_initial(key));
            assert_eq!(storage.read_value(key), *value);
        }
    }

    // Overwrite some of the logs.
    for log in storage_logs.values_mut().step_by(2) {
        *log = StorageValue::zero();
    }
    let changed_keys = RocksdbStorage::process_transaction_logs(&storage.db, storage_logs.clone());
    storage.pending_patch.state = changed_keys
        .into_iter()
        .map(|(key, state_value)| (key.hashed_key(), (state_value.value, 1))) // enum index doesn't matter in the test
        .collect();
    storage.save(Some(L1BatchNumber(1))).await.unwrap();

    for (key, value) in &storage_logs {
        assert!(!storage.is_write_initial(key));
        assert_eq!(storage.read_value(key), *value);
    }
}

async fn sync_test_storage(dir: &TempDir, conn: &mut StorageProcessor<'_>) -> RocksdbStorage {
    let (_stop_sender, stop_receiver) = watch::channel(false);
    RocksdbStorage::builder(dir.path())
        .await
        .expect("Failed initializing RocksDB")
        .synchronize(conn, &stop_receiver)
        .await
        .unwrap()
        .expect("Storage synchronization unexpectedly stopped")
}

#[tokio::test]
async fn rocksdb_storage_syncing_with_postgres() {
    let pool = ConnectionPool::test_pool().await;
    let mut conn = pool.access_storage().await.unwrap();
    prepare_postgres(&mut conn).await;
    let storage_logs = gen_storage_logs(20..40);
    create_miniblock(&mut conn, MiniblockNumber(1), storage_logs.clone()).await;
    create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &mut conn).await;

    assert_eq!(storage.l1_batch_number().await, Some(L1BatchNumber(2)));
    for log in &storage_logs {
        assert_eq!(storage.read_value(&log.key), log.value);
    }
}

async fn insert_factory_deps(
    conn: &mut StorageProcessor<'_>,
    miniblock_number: MiniblockNumber,
    indices: impl Iterator<Item = u8>,
) {
    let factory_deps = indices
        .map(|i| (H256::repeat_byte(i), vec![i; 64]))
        .collect();
    conn.storage_dal()
        .insert_factory_deps(miniblock_number, &factory_deps)
        .await;
}

#[tokio::test]
async fn rocksdb_storage_revert() {
    let pool = ConnectionPool::test_pool().await;
    let mut conn = pool.access_storage().await.unwrap();
    prepare_postgres(&mut conn).await;
    let storage_logs = gen_storage_logs(20..40);
    create_miniblock(&mut conn, MiniblockNumber(1), storage_logs[..10].to_vec()).await;
    insert_factory_deps(&mut conn, MiniblockNumber(1), 0..1).await;
    create_miniblock(&mut conn, MiniblockNumber(2), storage_logs[10..].to_vec()).await;
    insert_factory_deps(&mut conn, MiniblockNumber(2), 1..3).await;
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
    create_miniblock(&mut conn, MiniblockNumber(3), new_storage_logs).await;
    insert_factory_deps(&mut conn, MiniblockNumber(3), 3..5).await;
    create_l1_batch(&mut conn, L1BatchNumber(2), &inserted_storage_logs).await;

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &mut conn).await;

    // Perform some sanity checks before the revert.
    assert_eq!(storage.l1_batch_number().await, Some(L1BatchNumber(3)));
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

    storage.rollback(&mut conn, L1BatchNumber(1)).await.unwrap();
    assert_eq!(storage.l1_batch_number().await, Some(L1BatchNumber(2)));
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

#[tokio::test]
async fn rocksdb_enum_index_migration() {
    let pool = ConnectionPool::test_pool().await;
    let mut conn = pool.access_storage().await.unwrap();
    prepare_postgres(&mut conn).await;
    let storage_logs = gen_storage_logs(20..40);
    create_miniblock(&mut conn, MiniblockNumber(1), storage_logs.clone()).await;
    create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;

    let enum_indices: HashMap<_, _> = conn
        .storage_logs_dedup_dal()
        .initial_writes_for_batch(L1BatchNumber(1))
        .await
        .into_iter()
        .collect();

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &mut conn).await;

    assert_eq!(storage.l1_batch_number().await, Some(L1BatchNumber(2)));
    // Check that enum indices are correct after syncing with Postgres.
    for log in &storage_logs {
        let expected_index = enum_indices[&log.key.hashed_key()];
        assert_eq!(
            storage.get_enumeration_index(&log.key),
            Some(expected_index)
        );
    }

    // Remove enum indices for some keys.
    let mut write_batch = storage.db.new_write_batch();
    for log in &storage_logs {
        write_batch.put_cf(
            StateKeeperColumnFamily::State,
            log.key.hashed_key().as_bytes(),
            log.value.as_bytes(),
        );
        write_batch.delete_cf(
            StateKeeperColumnFamily::State,
            RocksdbStorage::ENUM_INDEX_MIGRATION_CURSOR,
        );
    }
    storage.db.write(write_batch).unwrap();

    // Check that migration works as expected.
    let ordered_keys_to_migrate: Vec<StorageKey> = storage_logs
        .iter()
        .map(|log| log.key)
        .sorted_by_key(StorageKey::hashed_key)
        .collect();

    storage.enum_index_migration_chunk_size = 10;
    let start_from = storage.enum_migration_start_from().await;
    assert_eq!(start_from, Some(H256::zero()));

    // Migrate the first half.
    storage.save_missing_enum_indices(&mut conn).await;
    for key in ordered_keys_to_migrate.iter().take(10) {
        let expected_index = enum_indices[&key.hashed_key()];
        assert_eq!(storage.get_enumeration_index(key), Some(expected_index));
    }
    let non_migrated_state_value =
        RocksdbStorage::read_state_value(&storage.db, ordered_keys_to_migrate[10].hashed_key())
            .unwrap();
    assert!(non_migrated_state_value.enum_index.is_none());

    // Migrate the second half.
    storage.save_missing_enum_indices(&mut conn).await;
    for key in ordered_keys_to_migrate.iter().skip(10) {
        let expected_index = enum_indices[&key.hashed_key()];
        assert_eq!(storage.get_enumeration_index(key), Some(expected_index));
    }

    // 20 keys were processed but we haven't checked that no keys to migrate are left.
    let start_from = storage.enum_migration_start_from().await;
    assert!(start_from.is_some());

    // Check that migration will be marked as completed after the next iteration.
    storage.save_missing_enum_indices(&mut conn).await;
    let start_from = storage.enum_migration_start_from().await;
    assert!(start_from.is_none());
}

#[tokio::test]
async fn low_level_snapshot_recovery() {
    let pool = ConnectionPool::test_pool().await;
    let mut conn = pool.access_storage().await.unwrap();
    let (snapshot_recovery, mut storage_logs) =
        prepare_postgres_for_snapshot_recovery(&mut conn).await;

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = RocksdbStorage::new(dir.path().to_path_buf()).await.unwrap();
    let next_l1_batch = storage.ensure_ready(&mut conn).await.unwrap();
    assert_eq!(next_l1_batch, snapshot_recovery.l1_batch_number + 1);
    assert_eq!(
        storage.l1_batch_number().await,
        Some(snapshot_recovery.l1_batch_number + 1)
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
async fn recovering_from_snapshot_and_following_logs() {
    let pool = ConnectionPool::test_pool().await;
    let mut conn = pool.access_storage().await.unwrap();
    let (snapshot_recovery, mut storage_logs) =
        prepare_postgres_for_snapshot_recovery(&mut conn).await;

    // Add some more storage logs.
    let new_storage_logs = gen_storage_logs(500..600);
    create_miniblock(
        &mut conn,
        snapshot_recovery.miniblock_number + 1,
        new_storage_logs.clone(),
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
    create_miniblock(
        &mut conn,
        snapshot_recovery.miniblock_number + 2,
        updated_storage_logs.clone(),
    )
    .await;
    create_l1_batch(&mut conn, snapshot_recovery.l1_batch_number + 2, &[]).await;

    let dir = TempDir::new().expect("cannot create temporary dir for state keeper");
    let mut storage = sync_test_storage(&dir, &mut conn).await;

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
}
