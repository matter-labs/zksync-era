use test_casing::test_casing;
use tokio::sync::watch;
use zksync_health_check::HealthStatus;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_state::RocksdbStorage;
use zksync_types::vm::FastVmMode;

use super::*;
use crate::impls::VmPlayground;

async fn run_playground(
    pool: ConnectionPool<Core>,
    rocksdb_dir: &tempfile::TempDir,
    reset_state: bool,
) {
    let mut conn = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    if conn.blocks_dal().is_genesis_needed().await.unwrap() {
        insert_genesis_batch(&mut conn, &genesis_params)
            .await
            .unwrap();

        // Generate some batches and persist them in Postgres
        let mut accounts = [Account::random()];
        fund(&mut conn, &accounts).await;
        store_l1_batches(
            &mut conn,
            1..=1, // TODO: test on >1 batch
            genesis_params.base_system_contracts().hashes(),
            &mut accounts,
        )
        .await
        .unwrap();
    }

    let (playground, playground_tasks) = VmPlayground::new(
        pool.clone(),
        None,
        FastVmMode::Shadow,
        rocksdb_dir.path().to_str().unwrap().to_owned(),
        genesis_params.config().l2_chain_id,
        L1BatchNumber(0),
        reset_state,
    )
    .await
    .unwrap();

    let (stop_sender, stop_receiver) = watch::channel(false);
    let playground_io = playground.io().clone();
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
    let mut health_check = playground.health_check();

    let mut completed_batches = playground_io.subscribe_to_completed_batches();
    let task_handles = [
        tokio::spawn(playground_tasks.loader_task.run(stop_receiver.clone())),
        tokio::spawn(
            playground_tasks
                .output_handler_factory_task
                .run(stop_receiver.clone()),
        ),
        tokio::spawn(async move { playground.run(&stop_receiver).await }),
    ];
    // Wait until all batches are processed.
    completed_batches
        .wait_for(|&number| number == L1BatchNumber(1))
        .await
        .unwrap();
    health_check
        .wait_for(|health| {
            if !matches!(health.status(), HealthStatus::Ready) {
                return false;
            }
            let health_details = health.details().unwrap();
            assert_eq!(health_details["vm_mode"], "shadow");
            health_details["last_processed_batch"] == 1_u64
        })
        .await;

    // Check that playground I/O works correctly.
    assert_eq!(
        playground_io
            .latest_processed_batch(&mut conn)
            .await
            .unwrap(),
        L1BatchNumber(1)
    );
    // There's no batch #2 in storage
    assert_eq!(
        playground_io
            .last_ready_to_be_loaded_batch(&mut conn)
            .await
            .unwrap(),
        L1BatchNumber(1)
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
    run_playground(pool, &rocksdb_dir, reset_state).await;
}

#[tokio::test]
async fn resetting_playground_state() {
    let pool = ConnectionPool::test_pool().await;
    let rocksdb_dir = tempfile::TempDir::new().unwrap();
    run_playground(pool.clone(), &rocksdb_dir, false).await;

    // Manually catch up RocksDB to Postgres to ensure that resetting it is not trivial.
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut conn = pool.connection().await.unwrap();
    RocksdbStorage::builder(rocksdb_dir.path())
        .await
        .unwrap()
        .synchronize(&mut conn, &stop_receiver, None)
        .await
        .unwrap();

    run_playground(pool.clone(), &rocksdb_dir, true).await;
}
