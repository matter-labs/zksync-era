use assert_matches::assert_matches;
use test_casing::test_casing;
use tokio::sync::watch;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_state::OwnedStorage;
use zksync_types::{L2ChainId, StorageLogWithPreviousValue};
use zksync_vm_executor::batch::MainBatchExecutorFactory;

use super::*;
use crate::{
    storage::{PostgresLoader, StorageLoader},
    ConcurrentOutputHandlerFactory, L1BatchOutput, L2BlockOutput, OutputHandler, VmRunner,
};

#[derive(Debug, Clone)]
struct StorageWriterIo {
    last_processed_block: L2BlockNumber,
    last_processed_batch: Arc<watch::Sender<L1BatchNumber>>,
    pool: ConnectionPool<Core>,
    insert_protective_reads: bool,
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
        // ^ The assertion works because of `last_ready_to_be_loaded_batch()` implementation; it wouldn't hold if we allowed
        // to process multiple batches concurrently.
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
            .insert_initial_writes_non_sequential(l1_batch_number, &initial_writes)
            .await?;

        if self.insert_protective_reads {
            let protective_reads: Vec<_> = output
                .batch
                .final_execution_state
                .deduplicated_storage_logs
                .iter()
                .filter(|log_query| !log_query.is_write())
                .copied()
                .collect();
            conn.storage_logs_dedup_dal()
                .insert_protective_reads(l1_batch_number, &protective_reads)
                .await?;
        }

        self.last_processed_batch.send_replace(l1_batch_number);
        Ok(())
    }
}

#[async_trait]
impl OutputHandlerFactory for StorageWriterIo {
    async fn create_handler(
        &self,
        _system_env: SystemEnv,
        l1_batch_env: L1BatchEnv,
    ) -> anyhow::Result<Box<dyn OutputHandler>> {
        assert_eq!(l1_batch_env.number, self.batch() + 1);
        Ok(Box::new(self.clone()))
    }
}

/// Writes missing storage logs into Postgres by executing all transactions from it. Useful both for testing `VmRunner`,
/// and to fill the storage for multi-batch tests for other components.
pub(super) async fn write_storage_logs(pool: ConnectionPool<Core>, insert_protective_reads: bool) {
    let mut conn = pool.connection().await.unwrap();
    let sealed_batch = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("No L1 batches in storage");
    drop(conn);
    let io = Arc::new(StorageWriterIo {
        last_processed_batch: Arc::new(watch::channel(L1BatchNumber(0)).0),
        last_processed_block: L2BlockNumber(0),
        pool: pool.clone(),
        insert_protective_reads,
    });
    let mut processed_batch = io.last_processed_batch.subscribe();

    let loader = PostgresLoader::new(pool.clone(), L2ChainId::default())
        .await
        .unwrap();
    let loader = Arc::new(loader);
    let batch_executor = MainBatchExecutorFactory::<()>::new(false);
    let vm_runner = VmRunner::new(pool, io.clone(), loader, io, Box::new(batch_executor));
    let (stop_sender, stop_receiver) = watch::channel(false);
    let vm_runner_handle = tokio::spawn(async move { vm_runner.run(&stop_receiver).await });

    let wait_result = processed_batch
        .wait_for(|&number| number >= sealed_batch)
        .await;
    if wait_result.is_err() {
        // If vm runner finished with error then panic with it.
        if vm_runner_handle.is_finished() {
            vm_runner_handle.await.unwrap().unwrap();
        }
        // Otherwise panic with wait error.
        wait_result.unwrap();
    } else {
        stop_sender.send_replace(true);
        vm_runner_handle.await.unwrap().unwrap();
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn storage_writer_works(insert_protective_reads: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut conn, &genesis_params.clone().into())
        .await
        .unwrap();

    let mut accounts = [Account::random()];
    fund(&mut conn, &accounts).await;
    store_l1_batches(&mut conn, 1..=5, &genesis_params, &mut accounts)
        .await
        .unwrap();
    drop(conn);

    write_storage_logs(pool.clone(), insert_protective_reads).await;

    // Re-run the VM on all batches to check that storage logs are persisted correctly
    let (stop_sender, stop_receiver) = watch::channel(false);
    let io = Arc::new(RwLock::new(IoMock {
        current: L1BatchNumber(0),
        max: 5,
    }));
    let loader = PostgresLoader::new(pool.clone(), genesis_params.config().l2_chain_id)
        .await
        .unwrap();
    let loader = Arc::new(loader);

    // Check that the loader returns expected types of storage.
    let (_, batch_storage) = loader
        .load_batch(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("no batch loaded");
    if insert_protective_reads {
        assert_matches!(batch_storage, OwnedStorage::Snapshot(_));
    } else {
        assert_matches!(batch_storage, OwnedStorage::Postgres(_));
    }

    let (output_factory, output_factory_task) =
        ConcurrentOutputHandlerFactory::new(pool.clone(), io.clone(), TestOutputFactory::default());
    let output_factory_handle = tokio::spawn(output_factory_task.run(stop_receiver.clone()));
    let batch_executor = MainBatchExecutorFactory::<()>::new(false);
    let vm_runner = VmRunner::new(
        pool,
        io.clone(),
        loader,
        Arc::new(output_factory),
        Box::new(batch_executor),
    );

    let vm_runner_handle = tokio::spawn(async move { vm_runner.run(&stop_receiver).await });
    wait::for_batch_progressively(io, L1BatchNumber(5), TEST_TIMEOUT)
        .await
        .unwrap();
    stop_sender.send_replace(true);
    output_factory_handle.await.unwrap().unwrap();
    vm_runner_handle.await.unwrap().unwrap();
}
