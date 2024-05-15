use std::{collections::HashMap, ops, sync::Arc, time::Duration};

use async_trait::async_trait;
use backon::{ConstantBuilder, ExponentialBuilder, Retryable};
use rand::Rng;
use tempfile::TempDir;
use tokio::{
    runtime::Handle,
    sync::{watch, RwLock},
    task::JoinHandle,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{
    create_l1_batch_metadata, create_l2_block, create_l2_transaction, execute_l2_transaction,
    l1_batch_metadata_to_commitment_artifacts,
};
use zksync_state::{PgOrRocksdbStorage, PostgresStorage, ReadStorage, ReadStorageFactory};
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader},
    fee::TransactionExecutionMetrics,
    AccountTreeId, L1BatchNumber, L2ChainId, ProtocolVersionId, StorageKey, StorageLog,
    StorageLogKind, StorageValue, H160, H256,
};

use super::{BatchExecuteData, VmRunnerIo, VmRunnerStorage};

#[derive(Debug, Default)]
struct IoMock {
    current: L1BatchNumber,
    max: L1BatchNumber,
}

#[async_trait]
impl VmRunnerIo for Arc<RwLock<IoMock>> {
    fn name() -> &'static str {
        "io_mock"
    }

    async fn latest_processed_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(self.read().await.current)
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(self.read().await.max)
    }

    async fn mark_l1_batch_as_completed(
        &self,
        _conn: &mut Connection<'_, Core>,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        Ok(())
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
        io_mock: Arc<RwLock<IoMock>>,
    ) -> anyhow::Result<VmRunnerStorage<Arc<RwLock<IoMock>>>> {
        let (vm_runner_storage, task) = VmRunnerStorage::new(
            self.pool.clone(),
            self.db_dir.path().to_str().unwrap().to_owned(),
            io_mock,
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

impl<Io: VmRunnerIo> VmRunnerStorage<Io> {
    async fn load_batch_eventually(
        &self,
        number: L1BatchNumber,
    ) -> anyhow::Result<BatchExecuteData> {
        (|| async {
            self.load_batch(number)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Batch #{} is not available yet", number))
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    async fn access_storage_eventually(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        number: L1BatchNumber,
    ) -> anyhow::Result<PgOrRocksdbStorage<'_>> {
        (|| async {
            self.access_storage(stop_receiver, number)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("Storage for batch #{} is not available yet", number)
                })
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    async fn ensure_batch_unloads_eventually(&self, number: L1BatchNumber) -> anyhow::Result<()> {
        (|| async {
            Ok(anyhow::ensure!(
                self.load_batch(number).await?.is_none(),
                "Batch #{} is still available",
                number
            ))
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    async fn batch_stays_unloaded(&self, number: L1BatchNumber) -> bool {
        (|| async {
            self.load_batch(number)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Batch #{} is not available yet", number))
        })
        .retry(
            &ConstantBuilder::default()
                .with_delay(Duration::from_millis(100))
                .with_max_times(3),
        )
        .await
        .is_err()
    }
}

async fn store_l2_blocks(
    conn: &mut Connection<'_, Core>,
    numbers: ops::RangeInclusive<u32>,
    contract_hashes: BaseSystemContractsHashes,
) -> anyhow::Result<Vec<L1BatchHeader>> {
    let mut rng = rand::thread_rng();
    let mut batches = Vec::new();
    let mut l2_block_number = conn
        .blocks_dal()
        .get_last_sealed_l2_block_header()
        .await?
        .map(|m| m.number)
        .unwrap_or_default()
        + 1;
    for l1_batch_number in numbers {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let tx = create_l2_transaction(10, 100);
        conn.transactions_dal()
            .insert_transaction_l2(&tx, TransactionExecutionMetrics::default())
            .await?;
        let mut logs = Vec::new();
        let mut written_keys = Vec::new();
        for _ in 0..10 {
            let key = StorageKey::new(AccountTreeId::new(H160::random()), H256::random());
            let value = StorageValue::random();
            written_keys.push(key);
            logs.push(StorageLog {
                kind: StorageLogKind::Write,
                key,
                value,
            });
        }
        let mut factory_deps = HashMap::new();
        for _ in 0..10 {
            factory_deps.insert(H256::random(), rng.gen::<[u8; 32]>().into());
        }
        conn.storage_logs_dal()
            .insert_storage_logs(l2_block_number, &[(tx.hash(), logs)])
            .await?;
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(l1_batch_number, &written_keys)
            .await?;
        conn.factory_deps_dal()
            .insert_factory_deps(l2_block_number, &factory_deps)
            .await?;
        let mut new_l2_block = create_l2_block(l2_block_number.0);
        l2_block_number += 1;
        new_l2_block.base_system_contracts_hashes = contract_hashes;
        new_l2_block.l2_tx_count = 1;
        conn.blocks_dal().insert_l2_block(&new_l2_block).await?;
        let tx_result = execute_l2_transaction(tx);
        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                new_l2_block.number,
                &[tx_result],
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await?;

        // Insert a fictive L2 block at the end of the batch
        let fictive_l2_block = create_l2_block(l2_block_number.0);
        l2_block_number += 1;
        conn.blocks_dal().insert_l2_block(&fictive_l2_block).await?;

        let header = L1BatchHeader::new(
            l1_batch_number,
            l2_block_number.0 as u64 - 2, // Matches the first L2 block in the batch
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
            .await?;
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
            .await?;

        let metadata = create_l1_batch_metadata(l1_batch_number.0);
        conn.blocks_dal()
            .save_l1_batch_tree_data(l1_batch_number, &metadata.tree_data())
            .await?;
        conn.blocks_dal()
            .save_l1_batch_commitment_artifacts(
                l1_batch_number,
                &l1_batch_metadata_to_commitment_artifacts(&metadata),
            )
            .await?;
        batches.push(header);
    }

    Ok(batches)
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
    let batches = store_l2_blocks(
        &mut connection_pool.connection().await?,
        1u32..=10u32,
        genesis_params.base_system_contracts().hashes(),
    )
    .await?;

    let mut tester = VmRunnerTester::new(connection_pool.clone());
    let io_mock = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: 10.into(),
    }));
    let storage = tester.create_storage(io_mock.clone()).await?;
    // Check that existing batches are returned in the exact same order with the exact same data
    for batch in &batches {
        let batch_data = storage.load_batch_eventually(batch.number).await?;
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
        let (first_l2_block_number, _) = conn
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(batch.number)
            .await?
            .unwrap();
        let previous_l2_block_header = conn
            .blocks_dal()
            .get_l2_block_header(first_l2_block_number - 1)
            .await?
            .unwrap();
        let l2_block_header = conn
            .blocks_dal()
            .get_l2_block_header(first_l2_block_number)
            .await?
            .unwrap();
        assert_eq!(
            batch_data.l1_batch_env.first_l2_block.number,
            l2_block_header.number.0
        );
        assert_eq!(
            batch_data.l1_batch_env.first_l2_block.timestamp,
            l2_block_header.timestamp
        );
        assert_eq!(
            batch_data.l1_batch_env.first_l2_block.prev_block_hash,
            previous_l2_block_header.hash
        );
        let l2_blocks = conn
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(batch_data.l1_batch_env.number)
            .await?;
        assert_eq!(batch_data.l2_blocks, l2_blocks);
    }

    // "Mark" these batches as processed
    io_mock.write().await.current += batches.len() as u32;

    // All old batches should no longer be loadable
    for batch in batches {
        storage
            .ensure_batch_unloads_eventually(batch.number)
            .await?;
    }

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
    let io_mock = Arc::new(RwLock::new(IoMock::default()));
    let storage = tester.create_storage(io_mock.clone()).await?;
    // No batches available yet
    assert!(storage.load_batch(L1BatchNumber(1)).await?.is_none());

    // Generate one batch and persist it in Postgres
    store_l2_blocks(
        &mut connection_pool.connection().await?,
        1u32..=1u32,
        genesis_params.base_system_contracts().hashes(),
    )
    .await?;
    io_mock.write().await.max += 1;

    // Load batch and mark it as processed
    assert_eq!(
        storage
            .load_batch_eventually(L1BatchNumber(1))
            .await?
            .l1_batch_env
            .number,
        L1BatchNumber(1)
    );
    io_mock.write().await.current += 1;

    // No more batches after that
    assert!(storage.batch_stays_unloaded(L1BatchNumber(2)).await);

    // Generate one more batch and persist it in Postgres
    store_l2_blocks(
        &mut connection_pool.connection().await?,
        2u32..=2u32,
        genesis_params.base_system_contracts().hashes(),
    )
    .await?;
    io_mock.write().await.max += 1;

    // Load batch and mark it as processed

    assert_eq!(
        storage
            .load_batch_eventually(L1BatchNumber(2))
            .await?
            .l1_batch_env
            .number,
        L1BatchNumber(2)
    );
    io_mock.write().await.current += 1;

    // No more batches after that
    assert!(storage.batch_stays_unloaded(L1BatchNumber(3)).await);

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
    store_l2_blocks(
        &mut connection_pool.connection().await?,
        batch_range,
        genesis_params.base_system_contracts().hashes(),
    )
    .await?;

    let mut conn = connection_pool.connection().await?;
    let storage_logs = conn
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    let factory_deps = conn
        .factory_deps_dal()
        .dump_all_factory_deps_for_tests()
        .await;
    drop(conn);

    let (_sender, receiver) = watch::channel(false);
    let mut tester = VmRunnerTester::new(connection_pool.clone());
    let io_mock = Arc::new(RwLock::new(IoMock {
        current: 0.into(),
        max: 10.into(),
    }));
    let rt_handle = Handle::current();
    let handle = tokio::task::spawn_blocking(move || {
        let vm_runner_storage =
            rt_handle.block_on(async { tester.create_storage(io_mock.clone()).await.unwrap() });
        for i in 1..=10 {
            let mut conn = rt_handle.block_on(connection_pool.connection()).unwrap();
            let (_, last_l2_block_number) = rt_handle
                .block_on(
                    conn.blocks_dal()
                        .get_l2_block_range_of_l1_batch(L1BatchNumber(i)),
                )?
                .unwrap();
            let mut pg_storage =
                PostgresStorage::new(rt_handle.clone(), conn, last_l2_block_number, true);
            let mut vm_storage = rt_handle.block_on(async {
                vm_runner_storage
                    .access_storage_eventually(&receiver, L1BatchNumber(i))
                    .await
            })?;
            // Check that both storages have identical key-value pairs written in them
            for storage_log in &storage_logs {
                let storage_key =
                    StorageKey::new(AccountTreeId::new(storage_log.address), storage_log.key);
                assert_eq!(
                    pg_storage.read_value(&storage_key),
                    vm_storage.read_value(&storage_key)
                );
                assert_eq!(
                    pg_storage.get_enumeration_index(&storage_key),
                    vm_storage.get_enumeration_index(&storage_key)
                );
                assert_eq!(
                    pg_storage.is_write_initial(&storage_key),
                    vm_storage.is_write_initial(&storage_key)
                );
            }
            for hash in factory_deps.keys() {
                assert_eq!(
                    pg_storage.load_factory_dep(*hash),
                    vm_storage.load_factory_dep(*hash)
                );
            }
        }

        anyhow::Ok(())
    });
    handle.await??;

    Ok(())
}
