use std::{collections::HashMap, sync::Arc, time::Duration};

use tempfile::TempDir;
use tokio::sync::{watch, RwLock};
use zksync_dal::{ConnectionPool, Core};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_state_keeper::MainBatchExecutor;
use zksync_test_account::Account;
use zksync_types::L2ChainId;

use crate::{
    tests::{fund, store_l1_batches, wait, IoMock, TestOutputFactory},
    ConcurrentOutputHandlerFactory, VmRunner, VmRunnerStorage,
};

// Testing more than a one-batch scenario is pretty difficult as that requires storage to have
// completely valid state after each L2 block execution (current block number, hash, rolling txs
// hash etc written to the correct places). To achieve this we could run state keeper e2e but that
// is pretty difficult to set up.
//
// Instead, we rely on integration tests to verify the correctness of VM runner main process.
// FIXME: this test is hard to fix without correctly storing the L2 DA validator in DB
// #[tokio::test]
async fn process_one_batch() -> anyhow::Result<()> {
    let rocksdb_dir = TempDir::new()?;
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = connection_pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();
    let alice = Account::random();
    let bob = Account::random();
    let mut accounts = vec![alice, bob];
    fund(&connection_pool, &accounts).await;

    let batches = store_l1_batches(
        &mut conn,
        1..=1,
        genesis_params.base_system_contracts().hashes(),
        &mut accounts,
    )
    .await?;
    drop(conn);

    let io = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: 1,
    }));
    let (storage, task) = VmRunnerStorage::new(
        connection_pool.clone(),
        rocksdb_dir.path().to_str().unwrap().to_owned(),
        io.clone(),
        L2ChainId::default(),
    )
    .await?;
    let (_, stop_receiver) = watch::channel(false);
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
        Box::new(batch_executor),
    );
    tokio::task::spawn(async move { vm_runner.run(&stop_receiver).await.unwrap() });

    for batch in batches {
        wait::for_batch(io.clone(), batch.number, Duration::from_secs(1)).await?;
    }

    Ok(())
}
