use std::{ops, sync::Arc};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::{
    runtime::Handle,
    sync::{watch, RwLock},
    task::JoinHandle,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state::{PostgresStorage, ReadStorage, ReadStorageFactory};
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader},
    fee::TransactionExecutionMetrics,
    AccountTreeId, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, StorageKey,
};

use super::{VmRunnerStorage, VmRunnerStorageLoader};
use crate::{
    genesis::{insert_genesis_batch, GenesisParams},
    utils::testonly::{
        create_l1_batch_metadata, create_l2_transaction, create_miniblock, execute_l2_transaction,
        l1_batch_metadata_to_commitment_artifacts,
    },
};

#[derive(Debug, Default)]
struct LoaderMock(L1BatchNumber);

#[async_trait]
impl VmRunnerStorageLoader for Arc<RwLock<LoaderMock>> {
    fn name() -> &'static str {
        "loader_mock"
    }

    async fn first_unprocessed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<Option<L1BatchNumber>> {
        let l1_batch_number = self.read().await.0 + 1;
        if conn
            .blocks_dal()
            .get_storage_l1_batch(l1_batch_number)
            .await?
            .is_some()
        {
            Ok(Some(l1_batch_number))
        } else {
            Ok(None)
        }
    }

    async fn latest_processed_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(self.read().await.0)
    }
}

#[derive(Debug)]
struct VmRunnerTester {
    db_dir: TempDir,
    pool: ConnectionPool<Core>,
    tasks: Vec<JoinHandle<()>>,
}

impl VmRunnerTester {
    fn new(pool: ConnectionPool<Core>) -> Self {
        Self {
            db_dir: TempDir::new().unwrap(),
            pool,
            tasks: Vec::new(),
        }
    }

    async fn create_storage(
        &mut self,
        loader_mock: Arc<RwLock<LoaderMock>>,
    ) -> anyhow::Result<VmRunnerStorage<Arc<RwLock<LoaderMock>>>> {
        let (vm_runner_storage, task) = VmRunnerStorage::new(
            self.pool.clone(),
            self.db_dir.path().to_str().unwrap().to_owned(),
            loader_mock,
            100,
            300000,
            L2ChainId::from(270),
        )
        .await?;
        let handle = tokio::task::spawn(async move {
            let (_stop_sender, stop_receiver) = watch::channel(false);
            task.run(stop_receiver).await.unwrap()
        });
        self.tasks.push(handle);
        Ok(vm_runner_storage)
    }
}

async fn store_miniblocks(
    conn: &mut Connection<'_, Core>,
    numbers: ops::RangeInclusive<u32>,
    contract_hashes: BaseSystemContractsHashes,
) -> Vec<L1BatchHeader> {
    let mut batches = Vec::new();
    for l1_batch_number in numbers {
        // We use one miniblock per batch so the number coincides
        let miniblock_number = l1_batch_number;
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let tx = create_l2_transaction(10, 100);
        conn.transactions_dal()
            .insert_transaction_l2(&tx, TransactionExecutionMetrics::default())
            .await
            .unwrap();
        let mut new_miniblock = create_miniblock(miniblock_number);
        new_miniblock.base_system_contracts_hashes = contract_hashes;
        conn.blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await
            .unwrap();
        let tx_result = execute_l2_transaction(tx);
        conn.transactions_dal()
            .mark_txs_as_executed_in_miniblock(new_miniblock.number, &[tx_result], 1.into())
            .await
            .unwrap();

        let header = L1BatchHeader::new(
            l1_batch_number,
            l1_batch_number.0 as u64,
            BaseSystemContractsHashes::default(),
            ProtocolVersionId::default(),
        );
        let predicted_gas = BlockGasCount {
            commit: 2,
            prove: 3,
            execute: 10,
        };
        conn.blocks_dal()
            .insert_l1_batch(&header, &[], predicted_gas, &[], &[], Default::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(l1_batch_number)
            .await
            .unwrap();

        let metadata = create_l1_batch_metadata(l1_batch_number.0);
        conn.blocks_dal()
            .save_l1_batch_tree_data(l1_batch_number, &metadata.tree_data())
            .await
            .unwrap();
        conn.blocks_dal()
            .save_l1_batch_commitment_artifacts(
                l1_batch_number,
                &l1_batch_metadata_to_commitment_artifacts(&metadata),
            )
            .await
            .unwrap();
        batches.push(header);
    }

    batches
}

#[tokio::test]
async fn rerun_storage_on_existing_data() -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = connection_pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();
    drop(conn);

    // Generate 10 batches worth of data and persist it in Postgres
    let batch_range = 1u32..=10u32;
    let batches = store_miniblocks(
        &mut connection_pool.connection().await?,
        batch_range,
        genesis_params.base_system_contracts().hashes(),
    )
    .await;

    let mut tester = VmRunnerTester::new(connection_pool.clone());
    let loader_mock = Arc::new(RwLock::new(LoaderMock::default()));
    let storage = tester.create_storage(loader_mock.clone()).await?;
    // Check that existing batches are returned in the exact same order with the exact same data
    for batch in batches {
        let batch_data = storage.load_next_batch().await?.unwrap();
        let mut conn = connection_pool.connection().await.unwrap();
        let (previous_batch_hash, _) = conn
            .blocks_dal()
            .get_l1_batch_state_root_and_timestamp(batch_data.l1_batch_env.number - 1)
            .await?
            .unwrap();
        assert_eq!(
            batch_data.l1_batch_env.previous_batch_hash,
            Some(previous_batch_hash)
        );
        assert_eq!(batch_data.l1_batch_env.number, batch.number);
        assert_eq!(batch_data.l1_batch_env.timestamp, batch.timestamp);
        let previous_miniblock_header = conn
            .blocks_dal()
            .get_miniblock_header(MiniblockNumber(batch_data.l1_batch_env.number.0 - 1))
            .await?
            .unwrap();
        let miniblock_header = conn
            .blocks_dal()
            .get_miniblock_header(batch_data.l1_batch_env.number.0.into())
            .await?
            .unwrap();
        assert_eq!(
            batch_data.l1_batch_env.first_l2_block.number,
            miniblock_header.number.0
        );
        assert_eq!(
            batch_data.l1_batch_env.first_l2_block.timestamp,
            miniblock_header.timestamp
        );
        assert_eq!(
            batch_data.l1_batch_env.first_l2_block.prev_block_hash,
            previous_miniblock_header.hash
        );
        let miniblocks = conn
            .transactions_dal()
            .get_miniblocks_to_execute_for_l1_batch(batch_data.l1_batch_env.number)
            .await?;
        assert_eq!(batch_data.miniblocks, miniblocks);

        // "Mark" this batch as processed
        loader_mock.write().await.0 += 1;
    }

    // No more batches to load after the pre-generated ones are exhausted
    assert!(storage.load_next_batch().await?.is_none());

    Ok(())
}

#[tokio::test]
async fn continuously_load_new_batches() -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = connection_pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();
    drop(conn);

    let mut tester = VmRunnerTester::new(connection_pool.clone());
    let loader_mock = Arc::new(RwLock::new(LoaderMock::default()));
    let storage = tester.create_storage(loader_mock.clone()).await?;
    // No batches available yet
    assert!(storage.load_next_batch().await?.is_none());

    // Generate one batch and persist it in Postgres
    store_miniblocks(
        &mut connection_pool.connection().await?,
        1u32..=1u32,
        genesis_params.base_system_contracts().hashes(),
    )
    .await;

    // Load batch and mark it as processed
    assert!(storage.load_next_batch().await?.is_some());
    loader_mock.write().await.0 += 1;

    // No more batches after that
    assert!(storage.load_next_batch().await?.is_none());

    // Generate one more batch and persist it in Postgres
    store_miniblocks(
        &mut connection_pool.connection().await?,
        2u32..=2u32,
        genesis_params.base_system_contracts().hashes(),
    )
    .await;

    // Load batch and mark it as processed
    assert!(storage.load_next_batch().await?.is_some());
    loader_mock.write().await.0 += 1;

    // No more batches after that
    assert!(storage.load_next_batch().await?.is_none());

    Ok(())
}

#[tokio::test]
async fn access_vm_runner_storage() -> anyhow::Result<()> {
    let connection_pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = connection_pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();
    drop(conn);

    // Generate 10 batches worth of data and persist it in Postgres
    let batch_range = 1u32..=10u32;
    store_miniblocks(
        &mut connection_pool.connection().await?,
        batch_range,
        genesis_params.base_system_contracts().hashes(),
    )
    .await;

    let storage_logs = connection_pool
        .connection()
        .await
        .unwrap()
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;

    let (_sender, receiver) = watch::channel(false);
    let mut tester = VmRunnerTester::new(connection_pool.clone());
    let loader_mock = Arc::new(RwLock::new(LoaderMock::default()));
    let rt_handle = Handle::current();
    let handle = tokio::task::spawn_blocking(move || {
        let conn = rt_handle.block_on(connection_pool.connection()).unwrap();
        let mut pg_storage =
            PostgresStorage::new(rt_handle.clone(), conn, MiniblockNumber(10), true);
        let vm_runner_storage =
            rt_handle.block_on(async { tester.create_storage(loader_mock.clone()).await.unwrap() });
        let mut vm_storage = rt_handle.block_on(async {
            vm_runner_storage
                .access_storage(&receiver, L1BatchNumber(10))
                .await
                .unwrap()
                .unwrap()
        });
        // Check that both storages have identical key-value pairs written in them
        for storage_log in storage_logs {
            let storage_key =
                StorageKey::new(AccountTreeId::new(storage_log.address), storage_log.key);
            let expected = pg_storage.read_value(&storage_key);
            let actual = vm_storage.read_value(&storage_key);
            assert_eq!(expected, actual);
        }

        anyhow::Ok(())
    });
    handle.await??;

    Ok(())
}
