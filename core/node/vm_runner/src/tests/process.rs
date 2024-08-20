use std::{collections::HashMap, sync::Arc};

use tempfile::TempDir;
use test_casing::test_casing;
use tokio::sync::{watch, RwLock};
use zksync_dal::{ConnectionPool, Core};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_test_account::Account;
use zksync_types::{L1BatchNumber, L2ChainId};
use zksync_vm_interface::executor;
use zksync_vm_utils::batch::MainBatchExecutor;

use super::*;
use crate::{ConcurrentOutputHandlerFactory, VmRunner, VmRunnerStorage};

#[test_casing(4, [(1, 1), (5, 1), (5, 3), (5, 5)])]
#[tokio::test(flavor = "multi_thread")]
async fn process_batches((batch_count, window): (u32, u32)) -> anyhow::Result<()> {
    let rocksdb_dir = TempDir::new()?;
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = connection_pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();
    let mut accounts = vec![Account::random(), Account::random()];
    fund(&mut conn, &accounts).await;

    store_l1_batches(
        &mut conn,
        1..=batch_count,
        genesis_params.base_system_contracts().hashes(),
        &mut accounts,
    )
    .await?;
    drop(conn);

    // Fill in missing storage logs for all batches so that running VM for all of them works correctly.
    storage_writer::write_storage_logs(connection_pool.clone()).await;

    let io = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: window,
    }));
    let (storage, task) = VmRunnerStorage::new(
        connection_pool.clone(),
        rocksdb_dir.path().to_str().unwrap().to_owned(),
        io.clone(),
        L2ChainId::default(),
    )
    .await?;
    let (_stop_sender, stop_receiver) = watch::channel(false);
    let storage_stop_receiver = stop_receiver.clone();
    tokio::task::spawn(async move { task.run(storage_stop_receiver).await.unwrap() });
    let test_factory = TestOutputFactory {
        delays: HashMap::new(),
    };
    let (output_factory, task) =
        ConcurrentOutputHandlerFactory::new(connection_pool.clone(), io.clone(), test_factory);
    let output_stop_receiver = stop_receiver.clone();
    tokio::task::spawn(async move { task.run(output_stop_receiver).await.unwrap() });

    let storage = Arc::new(storage);
    let batch_executor = MainBatchExecutor::new(false, false);
    let vm_runner = VmRunner::new(
        connection_pool,
        Box::new(io.clone()),
        storage,
        Box::new(output_factory),
        executor::box_batch_executor(batch_executor),
    );
    tokio::task::spawn(async move { vm_runner.run(&stop_receiver).await.unwrap() });

    wait::for_batch_progressively(io, L1BatchNumber(batch_count), TEST_TIMEOUT).await?;
    Ok(())
}
