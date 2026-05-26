use std::num::NonZeroU32;

use test_casing::test_casing;
use tokio::sync::watch;
use zksync_health_check::HealthStatus;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_state::RocksdbStorage;
use zksync_types::vm::FastVmMode;

use super::*;
use crate::impls::{
    VmPlayground, VmPlaygroundCursorOptions, VmPlaygroundStorageOptions, VmPlaygroundTasks,
};

impl From<&tempfile::TempDir> for VmPlaygroundStorageOptions {
    fn from(dir: &tempfile::TempDir) -> Self {
        Self::Rocksdb(dir.path().to_str().unwrap().into())
    }
}

async fn setup_storage(
    pool: &ConnectionPool<Core>,
    batch_count: u32,
    insert_protective_reads: bool,
) -> GenesisParams {
    let mut conn = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    if !conn.blocks_dal().is_genesis_needed().await.unwrap() {
        return genesis_params;
    }

    insert_genesis_batch(&mut conn, &genesis_params.clone().into())
        .await
        .unwrap();

    // Generate some batches and persist them in Postgres
    let mut accounts = [Account::random()];
    fund(&mut conn, &accounts).await;
    store_l1_batches(&mut conn, 1..=batch_count, &genesis_params, &mut accounts)
        .await
        .unwrap();

    // Fill in missing storage logs for all batches so that running VM for all of them works correctly.
    storage_writer::write_storage_logs(pool.clone(), insert_protective_reads).await;
    genesis_params
}

#[derive(Debug, Clone, Copy)]
enum StorageLoaderKind {
    Cached,
    Postgres,
    Snapshot,
}

impl StorageLoaderKind {
    const ALL: [Self; 3] = [Self::Cached, Self::Postgres, Self::Snapshot];
}

async fn run_playground(
    pool: ConnectionPool<Core>,
    storage: VmPlaygroundStorageOptions,
    reset_to: Option<L1BatchNumber>,
) {
    let insert_protective_reads = matches!(
        storage,
        VmPlaygroundStorageOptions::Snapshots { shadow: true }
    );
    let genesis_params = setup_storage(&pool, 5, insert_protective_reads).await;
    let cursor = VmPlaygroundCursorOptions {
        first_processed_batch: reset_to.unwrap_or(L1BatchNumber(0)),
        window_size: NonZeroU32::new(1).unwrap(),
        reset_state: reset_to.is_some(),
    };

    let (playground, playground_tasks) = VmPlayground::new(
        pool.clone(),
        None,
        FastVmMode::Shadow,
        storage,
        genesis_params.config().l2_chain_id,
        cursor,
    )
    .await
    .unwrap();

    let playground_io = playground.io().clone();
    let mut conn = pool.connection().await.unwrap();
    if reset_to.is_none() {
        assert_eq!(
            playground_io
                .latest_processed_batch(&mut conn)
                .await
                .unwrap(),
            L1BatchNumber(0)
        );
        assert_eq!(
            playground_io
                .last_ready_to_be_loaded_batch(&mut conn)
                .await
                .unwrap(),
            L1BatchNumber(1)
        );
    }

    wait_for_all_batches(playground, playground_tasks, &mut conn).await;
}

async fn wait_for_all_batches(
    playground: VmPlayground,
    playground_tasks: VmPlaygroundTasks,
    conn: &mut Connection<'_, Core>,
) {
    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut health_check = playground.health_check();
    let playground_io = playground.io().clone();

    let mut completed_batches = playground_io.subscribe_to_completed_batches();
    let mut task_handles = vec![
        tokio::spawn(
            playground_tasks
                .output_handler_factory_task
                .run(stop_receiver.clone()),
        ),
        tokio::spawn(playground.run(stop_receiver.clone())),
    ];
    if let Some(loader_task) = playground_tasks.loader_task {
        task_handles.push(tokio::spawn(loader_task.run(stop_receiver)));
    }

    // Wait until all batches are processed.
    let last_batch_number = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("No batches in storage");

    completed_batches
        .wait_for(|&number| number == last_batch_number)
        .await
        .unwrap();
    health_check
        .wait_for(|health| {
            if !matches!(health.status(), HealthStatus::Ready) {
                return false;
            }
            let health_details = health.details().unwrap();
            assert_eq!(health_details["vm_mode"], "shadow");
            health_details["last_processed_batch"] == u64::from(last_batch_number.0)
        })
        .await;

    // Check that playground I/O works correctly.
    assert_eq!(
        playground_io.latest_processed_batch(conn).await.unwrap(),
        last_batch_number
    );
    // There's no next batch
    assert_eq!(
        playground_io
            .last_ready_to_be_loaded_batch(conn)
            .await
            .unwrap(),
        last_batch_number
    );

    stop_sender.send_replace(true);
    for task_handle in task_handles {
        task_handle.await.unwrap().unwrap();
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn vm_playground_basics(reset_state: bool) {
    let pool = ConnectionPool::test_pool().await;
    let rocksdb_dir = tempfile::TempDir::new().unwrap();
    run_playground(
        pool,
        VmPlaygroundStorageOptions::from(&rocksdb_dir),
        reset_state.then_some(L1BatchNumber(0)),
    )
    .await;
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn vm_playground_basics_without_cache(reset_state: bool) {
    let pool = ConnectionPool::test_pool().await;
    run_playground(
        pool,
        VmPlaygroundStorageOptions::Snapshots { shadow: false },
        reset_state.then_some(L1BatchNumber(0)),
    )
    .await;
}

#[test_casing(3, StorageLoaderKind::ALL)]
#[tokio::test]
async fn starting_from_non_zero_batch(storage_loader_kind: StorageLoaderKind) {
    let pool = ConnectionPool::test_pool().await;
    let rocksdb_dir;
    let storage_loader = match storage_loader_kind {
        StorageLoaderKind::Cached => {
            rocksdb_dir = tempfile::TempDir::new().unwrap();
            VmPlaygroundStorageOptions::from(&rocksdb_dir)
        }
        StorageLoaderKind::Postgres => VmPlaygroundStorageOptions::Snapshots { shadow: false },
        StorageLoaderKind::Snapshot => VmPlaygroundStorageOptions::Snapshots { shadow: true },
    };
    run_playground(pool, storage_loader, Some(L1BatchNumber(3))).await;
}

#[test_casing(2, [L1BatchNumber(0), L1BatchNumber(2)])]
#[tokio::test]
async fn resetting_playground_state(reset_to: L1BatchNumber) {
    let pool = ConnectionPool::test_pool().await;
    let rocksdb_dir = tempfile::TempDir::new().unwrap();
    run_playground(
        pool.clone(),
        VmPlaygroundStorageOptions::from(&rocksdb_dir),
        None,
    )
    .await;

    // Manually catch up RocksDB to Postgres to ensure that resetting it is not trivial.
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut conn = pool.connection().await.unwrap();
    let rocksdb = RocksdbStorage::builder(rocksdb_dir.path())
        .await
        .unwrap()
        .get()
        .await
        .unwrap();
    rocksdb
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap();

    run_playground(
        pool.clone(),
        VmPlaygroundStorageOptions::from(&rocksdb_dir),
        Some(reset_to),
    )
    .await;
}

#[test_casing(2, [2, 3])]
#[tokio::test]
async fn using_larger_window_size(window_size: u32) {
    assert!(window_size > 1);
    let pool = ConnectionPool::test_pool().await;
    let rocksdb_dir = tempfile::TempDir::new().unwrap();

    let genesis_params = setup_storage(&pool, 5, false).await;
    let cursor = VmPlaygroundCursorOptions {
        first_processed_batch: L1BatchNumber(0),
        window_size: NonZeroU32::new(window_size).unwrap(),
        reset_state: false,
    };
    let (playground, playground_tasks) = VmPlayground::new(
        pool.clone(),
        None,
        FastVmMode::Shadow,
        VmPlaygroundStorageOptions::from(&rocksdb_dir),
        genesis_params.config().l2_chain_id,
        cursor,
    )
    .await
    .unwrap();

    let mut conn = pool.connection().await.unwrap();
    wait_for_all_batches(playground, playground_tasks, &mut conn).await;
}
