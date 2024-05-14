use std::{collections::HashMap, sync::Arc, time::Duration};

use tempfile::TempDir;
use tokio::sync::{watch, RwLock};
use zksync_core::state_keeper::MainBatchExecutor;
use zksync_dal::{ConnectionPool, Core};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_types::L2ChainId;

use crate::{
    storage::StorageLoader,
    tests::{store_l2_blocks, IoMock, TestOutputFactory},
    ConcurrentOutputHandlerFactory, VmRunner, VmRunnerStorage,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn simple() -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = connection_pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();

    // Generate 10 batches worth of data and persist it in Postgres
    let batches = store_l2_blocks(
        &mut conn,
        1u32..=10u32,
        genesis_params.base_system_contracts().hashes(),
    )
    .await?;
    drop(conn);

    let io_mock = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: 1,
    }));
    let (storage, task) = VmRunnerStorage::new(
        connection_pool.clone(),
        TempDir::new().unwrap().path().to_str().unwrap().to_owned(),
        io_mock.clone(),
        L2ChainId::from(270),
    )
    .await?;
    let (_, stop_receiver) = watch::channel(false);
    let storage_stop_receiver = stop_receiver.clone();
    tokio::task::spawn(async move { task.run(storage_stop_receiver).await.unwrap() });
    let test_factory = TestOutputFactory {
        delays: HashMap::new(),
    };
    let (output_factory, task) =
        ConcurrentOutputHandlerFactory::new(connection_pool.clone(), io_mock.clone(), test_factory);
    let output_stop_receiver = stop_receiver.clone();
    tokio::task::spawn(async move { task.run(output_stop_receiver).await.unwrap() });

    let storage = Arc::new(storage);
    let batch_executor = MainBatchExecutor::new(storage.clone(), false, false);
    let vm_runner = VmRunner::new(
        connection_pool,
        Box::new(io_mock),
        storage,
        Box::new(output_factory),
        Box::new(batch_executor),
    );
    tokio::task::spawn(async move { vm_runner.run(&stop_receiver).await.unwrap() });

    tokio::time::sleep(Duration::from_secs(10)).await;

    Ok(())
}
