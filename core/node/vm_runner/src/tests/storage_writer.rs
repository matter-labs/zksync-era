use tokio::sync::watch;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_types::StorageLogWithPreviousValue;
use zksync_vm_interface::executor;
use zksync_vm_utils::batch::MainBatchExecutorFactory;

use super::*;
use crate::{ConcurrentOutputHandlerFactory, VmRunner};

#[derive(Debug, Clone)]
struct StorageWriterIo {
    last_processed_block: L2BlockNumber,
    last_processed_batch: Arc<watch::Sender<L1BatchNumber>>,
    pool: ConnectionPool<Core>,
}

impl StorageWriterIo {
    fn batch(&self) -> L1BatchNumber {
        *self.last_processed_batch.borrow()
    }
}

#[async_trait]
impl VmRunnerIo for StorageWriterIo {
    fn name(&self) -> &'static str {
        "storage_writer"
    }

    async fn latest_processed_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(self.batch())
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        let sealed_batch = conn
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
            .expect("No L1 batches in storage");
        Ok(sealed_batch.min(self.batch() + 1))
    }

    async fn mark_l1_batch_as_processing(
        &self,
        _conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        assert_eq!(l1_batch_number, self.batch() + 1);
        Ok(())
    }

    async fn mark_l1_batch_as_completed(
        &self,
        _conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        assert_eq!(l1_batch_number, self.batch());
        Ok(())
    }
}

impl StorageWriterIo {
    async fn write_storage_logs(
        conn: &mut Connection<'_, Core>,
        block_number: L2BlockNumber,
        storage_logs: impl Iterator<Item = &StorageLogWithPreviousValue>,
    ) -> anyhow::Result<()> {
        let storage_logs = storage_logs.filter_map(|log| log.log.is_write().then_some(log.log));
        let storage_logs: Vec<_> = storage_logs.collect();
        conn.storage_logs_dal()
            .append_storage_logs(block_number, &storage_logs)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl OutputHandler for StorageWriterIo {
    async fn handle_l2_block(
        &mut self,
        env: L2BlockEnv,
        output: &L2BlockOutput,
    ) -> anyhow::Result<()> {
        let mut conn = self.pool.connection().await?;
        let storage_logs = output
            .transactions
            .iter()
            .flat_map(|(_, exec_result)| &exec_result.tx_result.logs.storage_logs);
        let block_number = L2BlockNumber(env.number);
        Self::write_storage_logs(&mut conn, block_number, storage_logs).await?;
        self.last_processed_block = block_number;
        Ok(())
    }

    async fn handle_l1_batch(self: Box<Self>, output: Arc<L1BatchOutput>) -> anyhow::Result<()> {
        let mut conn = self.pool.connection().await?;
        // Storage logs are added to the fictive block *after* `handle_l2_block()` is called for it, so we need to call it again here.
        let storage_logs = &output.batch.block_tip_execution_result.logs.storage_logs;
        Self::write_storage_logs(&mut conn, self.last_processed_block, storage_logs.iter()).await?;

        let state_diffs = output.batch.state_diffs.as_ref().expect("no state diffs");
        let initial_writes: Vec<_> = state_diffs
            .iter()
            .filter(|diff| diff.is_write_initial())
            .map(|diff| {
                H256(StorageKey::raw_hashed_key(
                    &diff.address,
                    &u256_to_h256(diff.key),
                ))
            })
            .collect();
        let l1_batch_number = *self.last_processed_batch.borrow() + 1;
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(l1_batch_number, &initial_writes)
            .await?;

        self.last_processed_batch.send_replace(l1_batch_number);
        Ok(())
    }
}

#[async_trait]
impl OutputHandlerFactory for StorageWriterIo {
    async fn create_handler(
        &mut self,
        _system_env: SystemEnv,
        l1_batch_env: L1BatchEnv,
    ) -> anyhow::Result<Box<dyn OutputHandler>> {
        assert_eq!(l1_batch_env.number, self.batch() + 1);
        Ok(Box::new(self.clone()))
    }
}

/// Writes missing storage logs into Postgres by executing all transactions from it. Useful both for testing `VmRunner`,
/// and to fill the storage for multi-batch tests for other components.
pub(super) async fn write_storage_logs(pool: ConnectionPool<Core>) {
    let mut conn = pool.connection().await.unwrap();
    let sealed_batch = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("No L1 batches in storage");
    drop(conn);
    let io = Box::new(StorageWriterIo {
        last_processed_batch: Arc::new(watch::channel(L1BatchNumber(0)).0),
        last_processed_block: L2BlockNumber(0),
        pool: pool.clone(),
    });
    let mut processed_batch = io.last_processed_batch.subscribe();

    let loader = Arc::new(PostgresLoader(pool.clone()));
    let batch_executor = MainBatchExecutorFactory::new(false, false);
    let vm_runner = VmRunner::new(
        pool,
        io.clone(),
        loader,
        io,
        executor::box_factory(batch_executor),
    );
    let (stop_sender, stop_receiver) = watch::channel(false);
    let vm_runner_handle = tokio::spawn(async move { vm_runner.run(&stop_receiver).await });

    processed_batch
        .wait_for(|&number| number >= sealed_batch)
        .await
        .unwrap();
    stop_sender.send_replace(true);
    vm_runner_handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn storage_writer_works() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params)
        .await
        .unwrap();

    let mut accounts = [Account::random()];
    fund(&mut conn, &accounts).await;
    store_l1_batches(
        &mut conn,
        1..=5,
        genesis_params.base_system_contracts().hashes(),
        &mut accounts,
    )
    .await
    .unwrap();
    drop(conn);

    write_storage_logs(pool.clone()).await;

    // Re-run the VM on all batches to check that storage logs are persisted correctly
    let (stop_sender, stop_receiver) = watch::channel(false);
    let io = Arc::new(RwLock::new(IoMock {
        current: L1BatchNumber(0),
        max: 5,
    }));
    let loader = Arc::new(PostgresLoader(pool.clone()));
    let (output_factory, output_factory_task) =
        ConcurrentOutputHandlerFactory::new(pool.clone(), io.clone(), TestOutputFactory::default());
    let output_factory_handle = tokio::spawn(output_factory_task.run(stop_receiver.clone()));
    let batch_executor = MainBatchExecutorFactory::new(false, false);
    let vm_runner = VmRunner::new(
        pool,
        Box::new(io.clone()),
        loader,
        Box::new(output_factory),
        executor::box_factory(batch_executor),
    );

    let vm_runner_handle = tokio::spawn(async move { vm_runner.run(&stop_receiver).await });
    wait::for_batch_progressively(io, L1BatchNumber(5), TEST_TIMEOUT)
        .await
        .unwrap();
    stop_sender.send_replace(true);
    output_factory_handle.await.unwrap().unwrap();
    vm_runner_handle.await.unwrap().unwrap();
}
