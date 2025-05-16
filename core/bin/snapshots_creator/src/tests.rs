//! Lower-level tests for the snapshot creator component.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use rand::{thread_rng, Rng};
use test_casing::test_casing;
use zksync_config::{ObjectStoreConfig, SnapshotsCreatorConfig};
use zksync_dal::{Connection, CoreDal};
use zksync_object_store::{MockObjectStore, ObjectStore};
use zksync_types::{
    block::{L1BatchHeader, L1BatchTreeData, L2BlockHeader},
    snapshots::{
        SnapshotFactoryDependencies, SnapshotFactoryDependency, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, ProtocolVersion, StorageKey, StorageLog,
    H256,
};

use super::*;

fn test_config() -> SnapshotsCreatorConfig {
    SnapshotsCreatorConfig {
        version: 1,
        l1_batch_number: None,
        storage_logs_chunk_size: 1_000_000,
        concurrent_queries_count: 10,
        object_store: ObjectStoreConfig::for_tests(),
    }
}

fn sequential_test_config() -> SnapshotsCreatorConfig {
    SnapshotsCreatorConfig {
        concurrent_queries_count: 1,
        ..test_config()
    }
}

#[derive(Debug)]
struct TestEventListener {
    stop_after_chunk_count: usize,
    processed_chunk_count: AtomicUsize,
}

impl TestEventListener {
    fn new(stop_after_chunk_count: usize) -> Self {
        Self {
            stop_after_chunk_count,
            processed_chunk_count: AtomicUsize::new(0),
        }
    }
}

impl HandleEvent for TestEventListener {
    fn on_chunk_started(&self) -> TestBehavior {
        let should_stop =
            self.processed_chunk_count.load(Ordering::SeqCst) >= self.stop_after_chunk_count;
        TestBehavior::new(should_stop)
    }

    fn on_chunk_saved(&self) {
        self.processed_chunk_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct UnreachableEventListener;

impl HandleEvent for UnreachableEventListener {
    fn on_chunk_started(&self) -> TestBehavior {
        unreachable!("should not be reached");
    }
}

impl SnapshotCreator {
    fn for_tests(blob_store: Arc<dyn ObjectStore>, pool: ConnectionPool<Core>) -> Self {
        Self {
            blob_store,
            master_pool: pool.clone(),
            replica_pool: pool,
            event_listener: Box::new(()),
        }
    }

    fn stop_after_chunk_count(self, stop_after_chunk_count: usize) -> Self {
        Self {
            event_listener: Box::new(TestEventListener::new(stop_after_chunk_count)),
            ..self
        }
    }

    fn panic_on_chunk_start(self) -> Self {
        Self {
            event_listener: Box::new(UnreachableEventListener),
            ..self
        }
    }
}

#[derive(Debug)]
pub(crate) struct TestBehavior {
    should_exit: bool,
}

impl TestBehavior {
    fn new(should_exit: bool) -> Self {
        Self { should_exit }
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }
}

pub(crate) trait HandleEvent: fmt::Debug {
    fn on_chunk_started(&self) -> TestBehavior {
        TestBehavior::new(false)
    }

    fn on_chunk_saved(&self) {
        // Do nothing
    }
}

impl HandleEvent for () {}

fn gen_storage_logs(rng: &mut impl Rng, count: usize) -> Vec<StorageLog> {
    (0..count)
        .map(|_| {
            let key = StorageKey::new(AccountTreeId::from_fixed_bytes(rng.gen()), H256(rng.gen()));
            StorageLog::new_write_log(key, H256(rng.gen()))
        })
        .collect()
}

fn gen_factory_deps(rng: &mut impl Rng, count: usize) -> HashMap<H256, Vec<u8>> {
    (0..count)
        .map(|_| {
            let factory_len = 32 * rng.gen_range(32..256);
            let mut factory = vec![0_u8; factory_len];
            rng.fill_bytes(&mut factory);
            (H256(rng.gen()), factory)
        })
        .collect()
}

#[derive(Debug, Default)]
struct ExpectedOutputs {
    deps: HashSet<SnapshotFactoryDependency>,
    storage_logs: HashSet<SnapshotStorageLog>,
}

async fn create_l2_block(
    conn: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
    block_logs: Vec<StorageLog>,
) {
    let l2_block_header = L2BlockHeader {
        number: l2_block_number,
        timestamp: 0,
        hash: H256::from_low_u64_be(u64::from(l2_block_number.0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::repeat_byte(1),
        base_fee_per_gas: 0,
        gas_per_pubdata_limit: 0,
        batch_fee_input: Default::default(),
        pubdata_params: Default::default(),
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(Default::default()),
        virtual_blocks: 0,
        gas_limit: 0,
        logs_bloom: Default::default(),
    };

    conn.blocks_dal()
        .insert_l2_block(&l2_block_header)
        .await
        .unwrap();
    conn.storage_logs_dal()
        .insert_storage_logs(l2_block_number, &block_logs)
        .await
        .unwrap();
}

async fn create_l1_batch(
    conn: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
    logs_for_initial_writes: &[StorageLog],
) {
    let header = L1BatchHeader::new(l1_batch_number, 0, Default::default(), Default::default());
    conn.blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    conn.blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
        .await
        .unwrap();

    let mut written_keys: Vec<_> = logs_for_initial_writes.iter().map(|log| log.key).collect();
    written_keys.sort_unstable();
    let written_keys: Vec<_> = written_keys.iter().map(StorageKey::hashed_key).collect();
    conn.storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch_number, &written_keys)
        .await
        .unwrap();
    conn.blocks_dal()
        .save_l1_batch_tree_data(
            l1_batch_number,
            &L1BatchTreeData {
                hash: H256::zero(),
                rollup_last_leaf_index: 1,
            },
        )
        .await
        .unwrap();
}

async fn prepare_postgres(
    rng: &mut impl Rng,
    conn: &mut Connection<'_, Core>,
    block_count: u32,
) -> ExpectedOutputs {
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    let mut outputs = ExpectedOutputs::default();
    for block_number in 0..block_count {
        let logs = gen_storage_logs(rng, 100);
        create_l2_block(conn, L2BlockNumber(block_number), logs.clone()).await;

        let factory_deps = gen_factory_deps(rng, 10);
        conn.factory_deps_dal()
            .insert_factory_deps(L2BlockNumber(block_number), &factory_deps)
            .await
            .unwrap();

        // Since we generate `logs` randomly, all of them are written the first time.
        create_l1_batch(conn, L1BatchNumber(block_number), &logs).await;

        if block_number + 1 < block_count {
            let factory_deps =
                factory_deps
                    .into_iter()
                    .map(|(hash, bytecode)| SnapshotFactoryDependency {
                        bytecode: bytecode.into(),
                        hash: Some(hash),
                    });
            outputs.deps.extend(factory_deps);

            let hashed_keys: Vec<_> = logs.iter().map(|log| log.key.hashed_key()).collect();
            let expected_l1_batches_and_indices = conn
                .storage_logs_dal()
                .get_l1_batches_and_indices_for_initial_writes(&hashed_keys)
                .await
                .unwrap();

            let logs = logs.into_iter().map(|log| {
                let (l1_batch_number_of_initial_write, enumeration_index) =
                    expected_l1_batches_and_indices[&log.key.hashed_key()];
                SnapshotStorageLog {
                    key: log.key.hashed_key(),
                    value: log.value,
                    l1_batch_number_of_initial_write,
                    enumeration_index,
                }
            });
            outputs.storage_logs.extend(logs);
        }
    }
    outputs
}

#[tokio::test]
async fn persisting_snapshot_metadata() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();

    // Insert some data to Postgres.
    let mut conn = pool.connection().await.unwrap();
    prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store, pool.clone())
        .run(test_config(), MIN_CHUNK_COUNT)
        .await
        .unwrap();

    // Check snapshot metadata in Postgres.
    let snapshots = conn
        .snapshots_dal()
        .get_all_complete_snapshots()
        .await
        .unwrap();
    assert_eq!(snapshots.snapshots_l1_batch_numbers.len(), 1);
    let snapshot_l1_batch_number = snapshots.snapshots_l1_batch_numbers[0];
    assert_eq!(snapshot_l1_batch_number, L1BatchNumber(8));

    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert_eq!(snapshot_metadata.l1_batch_number, snapshot_l1_batch_number);
    let factory_deps_path = &snapshot_metadata.factory_deps_filepath;
    assert!(factory_deps_path.ends_with(".proto.gzip"));
    assert_eq!(
        snapshot_metadata.storage_logs_filepaths.len(),
        MIN_CHUNK_COUNT as usize
    );
    for path in &snapshot_metadata.storage_logs_filepaths {
        let path = path
            .as_ref()
            .unwrap()
            .strip_prefix("storage_logs_snapshots/")
            .unwrap();
        assert!(path.ends_with(".proto.gzip"));
    }
}

#[tokio::test]
async fn persisting_snapshot_factory_deps() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(test_config(), MIN_CHUNK_COUNT)
        .await
        .unwrap();
    let snapshot_l1_batch_number = L1BatchNumber(8);

    let SnapshotFactoryDependencies { factory_deps } =
        object_store.get(snapshot_l1_batch_number).await.unwrap();
    let actual_deps: HashSet<_> = factory_deps.into_iter().collect();
    assert_eq!(actual_deps, expected_outputs.deps);
}

#[tokio::test]
async fn persisting_snapshot_logs() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(test_config(), MIN_CHUNK_COUNT)
        .await
        .unwrap();
    let snapshot_l1_batch_number = L1BatchNumber(8);

    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;
}

#[tokio::test]
async fn persisting_snapshot_logs_with_specified_l1_batch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    // L1 batch numbers are intentionally not ordered
    for snapshot_l1_batch_number in [7, 1, 4, 6] {
        let snapshot_l1_batch_number = L1BatchNumber(snapshot_l1_batch_number);
        let mut config = test_config();
        config.l1_batch_number = Some(snapshot_l1_batch_number);

        SnapshotCreator::for_tests(object_store.clone(), pool.clone())
            .run(config, MIN_CHUNK_COUNT)
            .await
            .unwrap();

        assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;
    }
}

async fn assert_storage_logs(
    object_store: &dyn ObjectStore,
    snapshot_l1_batch_number: L1BatchNumber,
    expected_outputs: &ExpectedOutputs,
) {
    let mut actual_logs = HashSet::new();
    for chunk_id in 0..MIN_CHUNK_COUNT {
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: snapshot_l1_batch_number,
            chunk_id,
        };
        let chunk: SnapshotStorageLogsChunk = object_store.get(key).await.unwrap();
        actual_logs.extend(chunk.storage_logs);
    }
    let expected_logs: HashSet<_> = expected_outputs
        .storage_logs
        .iter()
        .filter(|log| log.l1_batch_number_of_initial_write <= snapshot_l1_batch_number)
        .cloned()
        .collect();
    assert_eq!(actual_logs, expected_logs);
}

#[tokio::test]
async fn persisting_snapshot_logs_for_v0_snapshot() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    let config = SnapshotsCreatorConfig {
        version: 0,
        ..test_config()
    };
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(config, MIN_CHUNK_COUNT)
        .await
        .unwrap();
    let snapshot_l1_batch_number = L1BatchNumber(8);

    // Logs must be compatible with version 1 `SnapshotStorageLog` format
    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;

    // ...and must be compatible with version 0 format as well
    let mut actual_logs = HashSet::new();
    for chunk_id in 0..MIN_CHUNK_COUNT {
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: snapshot_l1_batch_number,
            chunk_id,
        };
        let chunk: SnapshotStorageLogsChunk<StorageKey> = object_store.get(key).await.unwrap();
        let logs_with_hashed_key = chunk
            .storage_logs
            .into_iter()
            .map(|log| SnapshotStorageLog {
                key: log.key.hashed_key(),
                value: log.value,
                l1_batch_number_of_initial_write: log.l1_batch_number_of_initial_write,
                enumeration_index: log.enumeration_index,
            });
        actual_logs.extend(logs_with_hashed_key);
    }
    assert_eq!(actual_logs, expected_outputs.storage_logs);
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn recovery_workflow(specify_batch_after_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .stop_after_chunk_count(0)
        .run(sequential_test_config(), MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let snapshot_l1_batch_number = L1BatchNumber(8);
    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert!(snapshot_metadata
        .storage_logs_filepaths
        .iter()
        .all(Option::is_none));

    let SnapshotFactoryDependencies { factory_deps } =
        object_store.get(snapshot_l1_batch_number).await.unwrap();
    let actual_deps: HashSet<_> = factory_deps.into_iter().collect();
    assert_eq!(actual_deps, expected_outputs.deps);

    // Process 2 storage log chunks, then stop.
    let recovery_config = SnapshotsCreatorConfig {
        l1_batch_number: specify_batch_after_recovery.then_some(snapshot_l1_batch_number),
        ..sequential_test_config()
    };
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .stop_after_chunk_count(2)
        .run(recovery_config.clone(), MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert_eq!(
        snapshot_metadata
            .storage_logs_filepaths
            .iter()
            .flatten()
            .count(),
        2
    );

    // Process the remaining chunks.
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(recovery_config.clone(), MIN_CHUNK_COUNT)
        .await
        .unwrap();

    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;

    // Check that the snapshot is not created anew after it is completed.
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .panic_on_chunk_start()
        .run(recovery_config, MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert!(snapshot_metadata.is_complete(), "{snapshot_metadata:#?}");
}

#[tokio::test]
async fn recovery_workflow_with_new_l1_batch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .stop_after_chunk_count(2)
        .run(sequential_test_config(), MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let snapshot_l1_batch_number = L1BatchNumber(8);
    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert!(!snapshot_metadata.is_complete(), "{snapshot_metadata:#?}");

    let new_logs = gen_storage_logs(&mut thread_rng(), 50);
    create_l1_batch(&mut conn, snapshot_l1_batch_number + 2, &new_logs).await;

    // The old snapshot should be completed.
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(sequential_test_config(), MIN_CHUNK_COUNT)
        .await
        .unwrap();
    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;

    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert!(snapshot_metadata.is_complete(), "{snapshot_metadata:#?}");
}

#[tokio::test]
async fn recovery_workflow_with_varying_chunk_size() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut rng = thread_rng();
    let object_store = MockObjectStore::arc();
    let mut conn = pool.connection().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    // Specifying the snapshot L1 batch right away should work fine.
    let snapshot_l1_batch_number = L1BatchNumber(8);
    let mut config = sequential_test_config();
    config.l1_batch_number = Some(snapshot_l1_batch_number);

    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .stop_after_chunk_count(2)
        .run(config.clone(), MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let snapshot_metadata = conn
        .snapshots_dal()
        .get_snapshot_metadata(snapshot_l1_batch_number)
        .await
        .unwrap()
        .expect("No snapshot metadata");
    assert_eq!(
        snapshot_metadata
            .storage_logs_filepaths
            .iter()
            .flatten()
            .count(),
        2
    );

    config.storage_logs_chunk_size = 1;
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(config, MIN_CHUNK_COUNT)
        .await
        .unwrap();

    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;
}

#[tokio::test]
async fn creator_fails_if_specified_l1_batch_is_missing() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let object_store = MockObjectStore::arc();

    let mut config = sequential_test_config();
    config.l1_batch_number = Some(L1BatchNumber(20));
    SnapshotCreator::for_tests(object_store.clone(), pool.clone())
        .run(config, MIN_CHUNK_COUNT)
        .await
        .unwrap_err();
}
