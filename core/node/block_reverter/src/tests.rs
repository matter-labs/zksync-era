//! Tests for block reverter.

use std::{collections::HashSet, sync::Mutex};

use assert_matches::assert_matches;
use async_trait::async_trait;
use test_casing::test_casing;
use tokio::sync::watch;
use zksync_dal::Connection;
use zksync_merkle_tree::TreeInstruction;
use zksync_object_store::{Bucket, MockObjectStore};
use zksync_state::interface::ReadStorage;
use zksync_types::{
    block::{L1BatchHeader, L2BlockHeader},
    fee_model::BatchFeeInput,
    snapshots::SnapshotVersion,
    AccountTreeId, L2BlockNumber, ProtocolVersion, ProtocolVersionId, StorageKey, StorageLog,
};

use super::*;

fn gen_storage_logs() -> Vec<StorageLog> {
    (0..10)
        .map(|i| {
            let key = StorageKey::new(AccountTreeId::default(), H256::from_low_u64_be(i));
            StorageLog::new_write_log(key, H256::repeat_byte(0xff))
        })
        .collect()
}

fn initialize_merkle_tree(path: &Path, storage_logs: &[StorageLog]) -> Vec<H256> {
    let db = RocksDB::new(path).unwrap().with_sync_writes();
    let mut tree = ZkSyncTree::new(db.into()).unwrap();
    let hashes = storage_logs.iter().enumerate().map(|(i, log)| {
        let output = tree
            .process_l1_batch(&[TreeInstruction::write(
                log.key.hashed_key_u256(),
                i as u64 + 1,
                log.value,
            )])
            .unwrap();
        tree.save().unwrap();
        output.root_hash
    });
    hashes.collect()
}

async fn setup_storage(storage: &mut Connection<'_, Core>, storage_logs: &[StorageLog]) {
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    for (number, storage_log) in (0..).zip(storage_logs) {
        let l2_block_header = L2BlockHeader {
            number: L2BlockNumber(number),
            timestamp: number.into(),
            hash: H256::from_low_u64_be(number.into()),
            l1_tx_count: 0,
            l2_tx_count: 0,
            fee_account_address: Address::default(),
            base_fee_per_gas: 0,
            batch_fee_input: BatchFeeInput::pubdata_independent(0, 0, 0),
            gas_per_pubdata_limit: 0,
            base_system_contracts_hashes: Default::default(),
            protocol_version: Some(ProtocolVersionId::latest()),
            virtual_blocks: 1,
            gas_limit: 0,
            logs_bloom: Default::default(),
            pubdata_params: Default::default(),
            rolling_txs_hash: None,
        };
        storage
            .blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();
        let l1_batch_header = L1BatchHeader {
            number: L1BatchNumber(number),
            timestamp: number.into(),
            l1_tx_count: 0,
            l2_tx_count: 0,
            priority_ops_onchain_data: vec![],
            l2_to_l1_logs: vec![],
            l2_to_l1_messages: vec![],
            bloom: Default::default(),
            used_contract_hashes: vec![],
            base_system_contracts_hashes: Default::default(),
            system_logs: vec![],
            protocol_version: Some(ProtocolVersionId::latest()),
            pubdata_input: None,
            fee_address: Default::default(),
            batch_fee_input: BatchFeeInput::pubdata_independent(0, 0, 0),
        };
        storage
            .blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_header.number)
            .await
            .unwrap();

        storage
            .storage_logs_dal()
            .insert_storage_logs(l2_block_header.number, &[*storage_log])
            .await
            .unwrap();
        storage
            .storage_logs_dedup_dal()
            .insert_initial_writes(l1_batch_header.number, &[storage_log.key.hashed_key()])
            .await
            .unwrap();
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn block_reverter_basics(sync_merkle_tree: bool) {
    let storage_logs = gen_storage_logs();
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    setup_storage(&mut storage, &storage_logs).await;

    let temp_dir = tempfile::tempdir().unwrap();
    let merkle_tree_path = temp_dir.path().join("tree");
    let storage_logs_for_merkle_tree = if sync_merkle_tree {
        &storage_logs
    } else {
        &storage_logs[..7] // include the target L1 batch #5, but don't process some newer batches
    };
    let l1_batch_hashes = initialize_merkle_tree(&merkle_tree_path, storage_logs_for_merkle_tree);
    for (number, hash) in (0..).zip(l1_batch_hashes) {
        storage
            .blocks_dal()
            .set_l1_batch_hash(L1BatchNumber(number), hash)
            .await
            .unwrap();
    }

    let sk_cache_path = temp_dir.path().join("sk_cache");
    let sk_cache = RocksdbStorage::builder(&sk_cache_path).await.unwrap();
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let (sk_cache, _) = sk_cache
        .ensure_ready(&pool, &stop_receiver)
        .await
        .unwrap()
        .expect("initialization interrupted");
    sk_cache
        .synchronize(&mut storage, &stop_receiver, None)
        .await
        .unwrap();

    BlockReverter::new(NodeRole::External, pool.clone())
        .enable_rolling_back_postgres()
        .enable_rolling_back_merkle_tree(merkle_tree_path.to_str().unwrap().to_owned())
        .add_rocksdb_storage_path_to_rollback(sk_cache_path.to_str().unwrap().to_owned())
        .roll_back(L1BatchNumber(5))
        .await
        .unwrap();

    let last_l1_batch_number = storage
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap();
    assert_eq!(last_l1_batch_number, Some(L1BatchNumber(5)));
    let last_l2_block_number = storage
        .blocks_dal()
        .get_sealed_l2_block_number()
        .await
        .unwrap();
    assert_eq!(last_l2_block_number, Some(L2BlockNumber(5)));

    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert_eq!(all_storage_logs.len(), 6);
    for (i, log) in all_storage_logs.iter().enumerate() {
        assert_eq!(log.l2_block_number, L2BlockNumber(i as u32));
        assert_eq!(log.value, H256::repeat_byte(0xff));
    }

    let mut all_initial_writes = storage
        .storage_logs_dedup_dal()
        .dump_all_initial_writes_for_tests()
        .await;
    assert_eq!(all_initial_writes.len(), 6);
    all_initial_writes.sort_unstable_by_key(|write| write.l1_batch_number);
    for (i, write) in all_initial_writes.iter().enumerate() {
        assert_eq!(write.l1_batch_number, L1BatchNumber(i as u32));
        assert_eq!(write.index, i as u64 + 1);
    }

    let db = RocksDB::new(&merkle_tree_path).unwrap();
    let tree = ZkSyncTree::new(db.into()).unwrap();
    assert_eq!(tree.next_l1_batch_number(), L1BatchNumber(6));

    let sk_cache = RocksdbStorage::builder(&sk_cache_path).await.unwrap();
    let sk_cache = sk_cache.get().await.unwrap();
    let mut sk_cache = sk_cache
        .synchronize(&mut storage, &stop_receiver, None)
        .await
        .unwrap()
        .expect("sk_cache syncing unexpectedly stopped");
    for (i, log) in storage_logs.iter().enumerate() {
        let expected_value = if i <= 5 { log.value } else { H256::zero() };
        assert_eq!(sk_cache.read_value(&log.key), expected_value);
    }
}

async fn create_mock_snapshot(
    storage: &mut Connection<'_, Core>,
    object_store: &dyn ObjectStore,
    l1_batch_number: L1BatchNumber,
    chunk_ids: impl Iterator<Item = u64> + Clone,
) {
    let storage_logs_chunk_count = chunk_ids
        .clone()
        .max()
        .expect("`chunk_ids` cannot be empty")
        + 1;

    let factory_deps_key = object_store
        .put(
            l1_batch_number,
            &SnapshotFactoryDependencies {
                factory_deps: vec![],
            },
        )
        .await
        .unwrap();
    storage
        .snapshots_dal()
        .add_snapshot(
            SnapshotVersion::Version0,
            l1_batch_number,
            storage_logs_chunk_count,
            &factory_deps_key,
        )
        .await
        .unwrap();

    for chunk_id in chunk_ids {
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number,
            chunk_id,
        };
        let key = object_store
            .put(
                key,
                &SnapshotStorageLogsChunk::<H256> {
                    storage_logs: vec![],
                },
            )
            .await
            .unwrap();
        storage
            .snapshots_dal()
            .add_storage_logs_filepath_for_snapshot(l1_batch_number, chunk_id, &key)
            .await
            .unwrap();
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn reverting_snapshot(remove_objects: bool) {
    let storage_logs = gen_storage_logs();
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    setup_storage(&mut storage, &storage_logs).await;

    let object_store = MockObjectStore::arc();
    create_mock_snapshot(&mut storage, &*object_store, L1BatchNumber(7), 0..5).await;
    // Sanity check: snapshot should be visible.
    let all_snapshots = storage
        .snapshots_dal()
        .get_all_complete_snapshots()
        .await
        .unwrap();
    assert_eq!(all_snapshots.snapshots_l1_batch_numbers, [L1BatchNumber(7)]);

    let mut block_reverter = BlockReverter::new(NodeRole::External, pool.clone());
    block_reverter.enable_rolling_back_postgres();
    if remove_objects {
        block_reverter.enable_rolling_back_snapshot_objects(object_store.clone());
    }
    block_reverter.roll_back(L1BatchNumber(5)).await.unwrap();

    // Check that snapshot has been removed.
    let all_snapshots = storage
        .snapshots_dal()
        .get_all_complete_snapshots()
        .await
        .unwrap();
    assert_eq!(all_snapshots.snapshots_l1_batch_numbers, []);

    let factory_deps_result = object_store
        .get::<SnapshotFactoryDependencies>(L1BatchNumber(7))
        .await;
    if remove_objects {
        assert_matches!(
            factory_deps_result.unwrap_err(),
            ObjectStoreError::KeyNotFound(_)
        );
    } else {
        factory_deps_result.unwrap();
    }

    for chunk_id in 0..5 {
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(7),
            chunk_id,
        };
        let chunk_result = object_store.get::<SnapshotStorageLogsChunk>(key).await;
        if remove_objects {
            assert_matches!(chunk_result.unwrap_err(), ObjectStoreError::KeyNotFound(_));
        } else {
            chunk_result.unwrap();
        }
    }
}

#[tokio::test]
async fn reverting_snapshot_ignores_not_found_object_store_errors() {
    let storage_logs = gen_storage_logs();
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    setup_storage(&mut storage, &storage_logs).await;

    let object_store = MockObjectStore::arc();
    create_mock_snapshot(&mut storage, &*object_store, L1BatchNumber(7), 0..5).await;

    // Manually remove some data from the store.
    object_store
        .remove::<SnapshotFactoryDependencies>(L1BatchNumber(7))
        .await
        .unwrap();
    let key = SnapshotStorageLogsStorageKey {
        l1_batch_number: L1BatchNumber(7),
        chunk_id: 1,
    };
    object_store
        .remove::<SnapshotStorageLogsChunk>(key)
        .await
        .unwrap();

    let mut block_reverter = BlockReverter::new(NodeRole::External, pool.clone());
    block_reverter.enable_rolling_back_postgres();
    block_reverter.enable_rolling_back_snapshot_objects(object_store);
    block_reverter.roll_back(L1BatchNumber(5)).await.unwrap();

    // Check that snapshot metadata has been removed.
    let all_snapshots = storage
        .snapshots_dal()
        .get_all_complete_snapshots()
        .await
        .unwrap();
    assert_eq!(all_snapshots.snapshots_l1_batch_numbers, []);
}

#[derive(Debug, Default)]
struct ErroneousStore {
    object_keys: Mutex<HashSet<(Bucket, String)>>,
}

#[async_trait]
impl ObjectStore for ErroneousStore {
    async fn get_raw(&self, _bucket: Bucket, _key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        unreachable!("not called by reverter")
    }

    async fn put_raw(
        &self,
        bucket: Bucket,
        key: &str,
        _value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        self.object_keys
            .lock()
            .unwrap()
            .insert((bucket, key.to_owned()));
        Ok(())
    }

    async fn remove_raw(&self, bucket: Bucket, key: &str) -> Result<(), ObjectStoreError> {
        self.object_keys
            .lock()
            .unwrap()
            .remove(&(bucket, key.to_owned()));
        Err(ObjectStoreError::Other {
            is_retriable: false,
            source: "fatal error".into(),
        })
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        bucket.to_string()
    }
}

#[tokio::test]
async fn reverting_snapshot_propagates_fatal_errors() {
    let storage_logs = gen_storage_logs();
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    setup_storage(&mut storage, &storage_logs).await;

    let object_store = Arc::new(ErroneousStore::default());
    create_mock_snapshot(&mut storage, &*object_store, L1BatchNumber(7), 0..5).await;

    let mut block_reverter = BlockReverter::new(NodeRole::External, pool.clone());
    block_reverter.enable_rolling_back_postgres();
    block_reverter.enable_rolling_back_snapshot_objects(object_store.clone());
    let err = block_reverter
        .roll_back(L1BatchNumber(5))
        .await
        .unwrap_err();
    assert!(err.chain().any(|source| {
        if let Some(err) = source.downcast_ref::<ObjectStoreError>() {
            matches!(err, ObjectStoreError::Other { .. })
        } else {
            false
        }
    }));

    // Check that snapshot metadata has been removed (it's not atomic with snapshot removal).
    let all_snapshots = storage
        .snapshots_dal()
        .get_all_complete_snapshots()
        .await
        .unwrap();
    assert_eq!(all_snapshots.snapshots_l1_batch_numbers, []);

    // Check that removal was called for all objects (i.e., the reverter doesn't bail early).
    let retained_object_keys = object_store.object_keys.lock().unwrap();
    assert!(retained_object_keys.is_empty(), "{retained_object_keys:?}");
}

#[tokio::test]
async fn reverter_handles_incomplete_snapshot() {
    let storage_logs = gen_storage_logs();
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    setup_storage(&mut storage, &storage_logs).await;

    let object_store = MockObjectStore::arc();
    let chunk_ids = [0, 1, 4].into_iter();
    create_mock_snapshot(
        &mut storage,
        &*object_store,
        L1BatchNumber(7),
        chunk_ids.clone(),
    )
    .await;

    let mut block_reverter = BlockReverter::new(NodeRole::External, pool.clone());
    block_reverter.enable_rolling_back_postgres();
    block_reverter.enable_rolling_back_snapshot_objects(object_store.clone());
    block_reverter.roll_back(L1BatchNumber(5)).await.unwrap();

    // Check that snapshot metadata has been removed.
    let all_snapshots = storage
        .snapshots_dal()
        .get_all_complete_snapshots()
        .await
        .unwrap();
    assert_eq!(all_snapshots.snapshots_l1_batch_numbers, []);

    // Check that chunk files have been removed.
    for chunk_id in chunk_ids {
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(7),
            chunk_id,
        };
        let chunk_result = object_store.get::<SnapshotStorageLogsChunk>(key).await;
        assert_matches!(chunk_result.unwrap_err(), ObjectStoreError::KeyNotFound(_));
    }
}
