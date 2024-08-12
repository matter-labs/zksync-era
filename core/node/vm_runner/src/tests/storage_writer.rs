use tokio::sync::watch;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_state_keeper::MainBatchExecutor;

use super::*;
use crate::{ConcurrentOutputHandlerFactory, VmRunner};

#[derive(Debug, Clone)]
struct StorageWriterIo {
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
        updates_manager: &UpdatesManager,
    ) -> anyhow::Result<()> {
        let storage_logs = updates_manager
            .l2_block
            .storage_logs
            .iter()
            .filter_map(|log| log.log.is_write().then_some(log.log));
        let storage_logs: Vec<_> = storage_logs.collect();
        conn.storage_logs_dal()
            .append_storage_logs(updates_manager.l2_block.number, &storage_logs)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl StateKeeperOutputHandler for StorageWriterIo {
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        let mut conn = self.pool.connection().await?;
        Self::write_storage_logs(&mut conn, updates_manager).await?;
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let mut conn = self.pool.connection().await?;
        // Storage logs are added to the fictive block *after* `handle_l2_block()` is called for it, so we need to call it again here.
        Self::write_storage_logs(&mut conn, &updates_manager).await?;

        let finished_batch = updates_manager
            .l1_batch
            .finished
            .as_ref()
            .expect("L1 batch is not finished");
        let state_diffs = finished_batch.state_diffs.as_ref().expect("no state diffs");
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
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(updates_manager.l1_batch.number, &initial_writes)
            .await?;

        self.last_processed_batch
            .send_replace(updates_manager.l1_batch.number);
        Ok(())
    }
}

#[async_trait]
impl OutputHandlerFactory for StorageWriterIo {
    async fn create_handler(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        assert_eq!(l1_batch_number, self.batch() + 1);
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
        pool: pool.clone(),
    });
    let mut processed_batch = io.last_processed_batch.subscribe();

    let loader = Arc::new(PostgresLoader(pool.clone()));
    let batch_executor = Box::new(MainBatchExecutor::new(false, false));
    let vm_runner = VmRunner::new(pool, io.clone(), loader, io, batch_executor);
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
    let batch_executor = Box::new(MainBatchExecutor::new(false, false));
    let vm_runner = VmRunner::new(
        pool,
        Box::new(io.clone()),
        loader,
        Box::new(output_factory),
        batch_executor,
    );

    let vm_runner_handle = tokio::spawn(async move { vm_runner.run(&stop_receiver).await });
    wait::for_batch_progressively(io, L1BatchNumber(5), TEST_TIMEOUT)
        .await
        .unwrap();
    stop_sender.send_replace(true);
    output_factory_handle.await.unwrap().unwrap();
    vm_runner_handle.await.unwrap().unwrap();
}
