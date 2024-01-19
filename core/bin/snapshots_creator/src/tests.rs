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
use zksync_dal::StorageProcessor;
use zksync_object_store::ObjectStore;
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    snapshots::{
        SnapshotFactoryDependencies, SnapshotFactoryDependency, SnapshotStorageLog,
        SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, ProtocolVersion, StorageKey,
    StorageLog, H256,
};

use super::*;

const TEST_CONFIG: SnapshotsCreatorConfig = SnapshotsCreatorConfig {
    storage_logs_chunk_size: 1_000_000,
    concurrent_queries_count: 10,
};
const SEQUENTIAL_TEST_CONFIG: SnapshotsCreatorConfig = SnapshotsCreatorConfig {
    storage_logs_chunk_size: 1_000_000,
    concurrent_queries_count: 1,
};

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

impl SnapshotCreator {
    fn for_tests(blob_store: Arc<dyn ObjectStore>, pool: ConnectionPool) -> Self {
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

async fn create_miniblock(
    conn: &mut StorageProcessor<'_>,
    miniblock_number: MiniblockNumber,
    block_logs: Vec<StorageLog>,
) {
    let miniblock_header = MiniblockHeader {
        number: miniblock_number,
        timestamp: 0,
        hash: H256::from_low_u64_be(u64::from(miniblock_number.0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 0,
        gas_per_pubdata_limit: 0,
        batch_fee_input: Default::default(),
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(Default::default()),
        virtual_blocks: 0,
    };

    conn.blocks_dal()
        .insert_miniblock(&miniblock_header)
        .await
        .unwrap();
    conn.storage_logs_dal()
        .insert_storage_logs(miniblock_number, &[(H256::zero(), block_logs)])
        .await;
}

async fn create_l1_batch(
    conn: &mut StorageProcessor<'_>,
    l1_batch_number: L1BatchNumber,
    logs_for_initial_writes: &[StorageLog],
) {
    let mut header = L1BatchHeader::new(
        l1_batch_number,
        0,
        Address::default(),
        Default::default(),
        Default::default(),
    );
    header.is_finished = true;
    conn.blocks_dal()
        .insert_l1_batch(&header, &[], BlockGasCount::default(), &[], &[], 0)
        .await
        .unwrap();
    conn.blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(l1_batch_number)
        .await
        .unwrap();

    let mut written_keys: Vec<_> = logs_for_initial_writes.iter().map(|log| log.key).collect();
    written_keys.sort_unstable();
    conn.storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch_number, &written_keys)
        .await;
}

async fn prepare_postgres(
    rng: &mut impl Rng,
    conn: &mut StorageProcessor<'_>,
    block_count: u32,
) -> ExpectedOutputs {
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;

    let mut outputs = ExpectedOutputs::default();
    for block_number in 0..block_count {
        let logs = gen_storage_logs(rng, 100);
        create_miniblock(conn, MiniblockNumber(block_number), logs.clone()).await;

        let factory_deps = gen_factory_deps(rng, 10);
        conn.storage_dal()
            .insert_factory_deps(MiniblockNumber(block_number), &factory_deps)
            .await;

        // Since we generate `logs` randomly, all of them are written the first time.
        create_l1_batch(conn, L1BatchNumber(block_number), &logs).await;

        if block_number + 1 < block_count {
            let factory_deps =
                factory_deps
                    .into_values()
                    .map(|bytecode| SnapshotFactoryDependency {
                        bytecode: bytecode.into(),
                    });
            outputs.deps.extend(factory_deps);

            let hashed_keys: Vec<_> = logs.iter().map(|log| log.key.hashed_key()).collect();
            let expected_l1_batches_and_indices = conn
                .storage_logs_dal()
                .get_l1_batches_and_indices_for_initial_writes(&hashed_keys)
                .await;

            let logs = logs.into_iter().map(|log| {
                let (l1_batch_number_of_initial_write, enumeration_index) =
                    expected_l1_batches_and_indices[&log.key.hashed_key()];
                SnapshotStorageLog {
                    key: log.key,
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
    let pool = ConnectionPool::test_pool().await;
    let mut rng = thread_rng();
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;

    // Insert some data to Postgres.
    let mut conn = pool.access_storage().await.unwrap();
    prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store, pool.clone())
        .run(TEST_CONFIG, MIN_CHUNK_COUNT)
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
    let pool = ConnectionPool::test_pool().await;
    let mut rng = thread_rng();
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let mut conn = pool.access_storage().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store, pool.clone())
        .run(TEST_CONFIG, MIN_CHUNK_COUNT)
        .await
        .unwrap();
    let snapshot_l1_batch_number = L1BatchNumber(8);

    let object_store = object_store_factory.create_store().await;
    let SnapshotFactoryDependencies { factory_deps } =
        object_store.get(snapshot_l1_batch_number).await.unwrap();
    let actual_deps: HashSet<_> = factory_deps.into_iter().collect();
    assert_eq!(actual_deps, expected_outputs.deps);
}

#[tokio::test]
async fn persisting_snapshot_logs() {
    let pool = ConnectionPool::test_pool().await;
    let mut rng = thread_rng();
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let mut conn = pool.access_storage().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store, pool.clone())
        .run(TEST_CONFIG, MIN_CHUNK_COUNT)
        .await
        .unwrap();
    let snapshot_l1_batch_number = L1BatchNumber(8);

    let object_store = object_store_factory.create_store().await;
    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;
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
        actual_logs.extend(chunk.storage_logs.into_iter());
    }
    assert_eq!(actual_logs, expected_outputs.storage_logs);
}

#[tokio::test]
async fn recovery_workflow() {
    let pool = ConnectionPool::test_pool().await;
    let mut rng = thread_rng();
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let mut conn = pool.access_storage().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store, pool.clone())
        .stop_after_chunk_count(0)
        .run(SEQUENTIAL_TEST_CONFIG, MIN_CHUNK_COUNT)
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

    let object_store = object_store_factory.create_store().await;
    let SnapshotFactoryDependencies { factory_deps } =
        object_store.get(snapshot_l1_batch_number).await.unwrap();
    let actual_deps: HashSet<_> = factory_deps.into_iter().collect();
    assert_eq!(actual_deps, expected_outputs.deps);

    // Process 2 storage log chunks, then stop.
    SnapshotCreator::for_tests(object_store, pool.clone())
        .stop_after_chunk_count(2)
        .run(SEQUENTIAL_TEST_CONFIG, MIN_CHUNK_COUNT)
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
    let object_store = object_store_factory.create_store().await;
    SnapshotCreator::for_tests(object_store, pool.clone())
        .run(SEQUENTIAL_TEST_CONFIG, MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let object_store = object_store_factory.create_store().await;
    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;
}

#[tokio::test]
async fn recovery_workflow_with_varying_chunk_size() {
    let pool = ConnectionPool::test_pool().await;
    let mut rng = thread_rng();
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let mut conn = pool.access_storage().await.unwrap();
    let expected_outputs = prepare_postgres(&mut rng, &mut conn, 10).await;

    SnapshotCreator::for_tests(object_store, pool.clone())
        .stop_after_chunk_count(2)
        .run(SEQUENTIAL_TEST_CONFIG, MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let snapshot_l1_batch_number = L1BatchNumber(8);
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

    let config_with_other_size = SnapshotsCreatorConfig {
        storage_logs_chunk_size: 1, // << should be ignored
        ..SEQUENTIAL_TEST_CONFIG
    };
    let object_store = object_store_factory.create_store().await;
    SnapshotCreator::for_tests(object_store, pool.clone())
        .run(config_with_other_size, MIN_CHUNK_COUNT)
        .await
        .unwrap();

    let object_store = object_store_factory.create_store().await;
    assert_storage_logs(&*object_store, snapshot_l1_batch_number, &expected_outputs).await;
}
