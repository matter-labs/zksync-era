use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::{
    sync::{watch, Mutex},
    task::JoinHandle,
};
use zksync_dal::{ConnectionPool, Core};
use zksync_state::OwnedStorage;
use zksync_types::L1BatchNumber;
use zksync_vm_interface::{executor::BatchExecutorFactory, L2BlockEnv};

use crate::{
    metrics::{StorageKind, METRICS},
    storage::StorageLoader,
    L1BatchOutput, L2BlockOutput, OutputHandlerFactory, VmRunnerIo,
};

const SLEEP_INTERVAL: Duration = Duration::from_millis(50);

/// VM runner represents a logic layer of L1 batch / L2 block processing flow akin to that of state keeper.
///
/// The difference is that VM runner is designed to be run on batches/blocks that have
/// already been processed by state keeper but still require some extra handling as regulated by
/// [`OutputHandlerFactory`].
///
/// It's responsible for taking unprocessed data from the [`VmRunnerIo`], feeding it into
/// [`BatchExecutor`] and calling [`OutputHandlerFactory`] on the result of the execution (batch
/// execution state in the [`UpdatesManager`]).
///
/// You can think of VM runner as a concurrent processor of a continuous stream of newly committed
/// batches/blocks.
#[derive(Debug, Clone)]
pub struct VmRunner {
    pool: ConnectionPool<Core>,
    io: Arc<dyn VmRunnerIo>,
    loader: Arc<dyn StorageLoader>,
    output_handler_factory: Arc<dyn OutputHandlerFactory>,
    batch_executor_factory: Arc<Mutex<Box<dyn BatchExecutorFactory<OwnedStorage>>>>,
}

impl VmRunner {
    /// Initializes VM runner with its constituents. In order to make VM runner concurrent each
    /// parameter here needs to support concurrent execution mode. See
    /// [`ConcurrentOutputHandlerFactory`], [`VmRunnerStorage`].
    ///
    /// Caller is expected to provide a component-specific implementation of [`VmRunnerIo`] and
    /// an underlying implementation of [`OutputHandlerFactory`].
    pub fn new(
        pool: ConnectionPool<Core>,
        io: Arc<dyn VmRunnerIo>,
        loader: Arc<dyn StorageLoader>,
        output_handler_factory: Arc<dyn OutputHandlerFactory>,
        batch_executor_factory: Box<dyn BatchExecutorFactory<OwnedStorage>>,
    ) -> Self {
        Self {
            pool,
            io,
            loader,
            output_handler_factory,
            batch_executor_factory: Arc::new(Mutex::new(batch_executor_factory)),
        }
    }

    #[tracing::instrument(skip(self), err, fields(runner = self.io.name()))]
    async fn process_batch(self, number: L1BatchNumber) -> anyhow::Result<()> {
        let stage_started_at = Instant::now();
        let (batch_data, storage) = loop {
            match self.loader.load_batch(number).await? {
                Some(data_and_storage) => break data_and_storage,
                None => {
                    // Next batch has not been loaded yet
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                }
            }
        };
        let kind = StorageKind::new(&storage);
        METRICS.data_and_storage_latency[&kind].observe(stage_started_at.elapsed());

        let mut batch_executor = self.batch_executor_factory.lock().await.init_batch(
            storage,
            batch_data.l1_batch_env.clone(),
            batch_data.system_env.clone(),
            batch_data.pubdata_params,
        );
        let mut output_handler = self
            .output_handler_factory
            .create_handler(batch_data.system_env, batch_data.l1_batch_env)
            .await?;
        self.io
            .mark_l1_batch_as_processing(
                &mut self.pool.connection_tagged("vm_runner").await?,
                number,
            )
            .await?;

        let latency = METRICS.run_vm_time.start();
        for (i, l2_block) in batch_data.l2_blocks.into_iter().enumerate() {
            let block_env = L2BlockEnv::from_l2_block_data(&l2_block);
            if i > 0 {
                // First L2 block in every batch is already preloaded
                batch_executor
                    .start_next_l2_block(block_env.clone())
                    .await
                    .with_context(|| {
                        format!("failed starting L2 block with {block_env:?} in batch executor")
                    })?;
            }

            let mut block_output = L2BlockOutput::default();
            for tx in l2_block.txs {
                let exec_result = batch_executor
                    .execute_tx(tx.clone())
                    .await
                    .with_context(|| format!("failed executing transaction {:?}", tx.hash()))?;
                anyhow::ensure!(
                    !exec_result.was_halted(),
                    "Unexpected non-successful transaction"
                );
                block_output.push(tx, exec_result);
            }
            output_handler
                .handle_l2_block(block_env, &block_output)
                .await
                .context("VM runner failed to handle L2 block")?;
        }

        let (batch, storage_view) = batch_executor
            .finish_batch()
            .await
            .context("VM runner failed to execute batch tip")?;
        let output = L1BatchOutput {
            batch,
            storage_view_cache: storage_view.cache(),
        };
        latency.observe();
        output_handler
            .handle_l1_batch(Arc::new(output))
            .await
            .context("VM runner failed to handle L1 batch")?;
        Ok(())
    }

    /// Consumes VM runner to execute a loop that continuously pulls data from [`VmRunnerIo`] and
    /// processes it.
    pub async fn run(self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        // Join handles for asynchronous tasks that are being run in the background
        let mut task_handles: Vec<(L1BatchNumber, JoinHandle<anyhow::Result<()>>)> = Vec::new();
        let mut next_batch = self
            .io
            .latest_processed_batch(&mut self.pool.connection_tagged("vm_runner").await?)
            .await?
            + 1;
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("VM runner was interrupted");
                return Ok(());
            }

            // Traverse all handles and filter out tasks that have been finished. Also propagates
            // any panic/error that might have happened during the task's execution.
            let mut retained_handles = Vec::new();
            for (l1_batch_number, handle) in task_handles {
                if handle.is_finished() {
                    handle
                        .await
                        .with_context(|| format!("Processing batch #{} panicked", l1_batch_number))?
                        .with_context(|| format!("Failed to process batch #{}", l1_batch_number))?;
                } else {
                    retained_handles.push((l1_batch_number, handle));
                }
            }
            task_handles = retained_handles;
            METRICS
                .in_progress_l1_batches
                .set(task_handles.len() as u64);

            let last_ready_batch = self
                .io
                .last_ready_to_be_loaded_batch(&mut self.pool.connection_tagged("vm_runner").await?)
                .await?;
            METRICS.last_ready_batch.set(last_ready_batch.0.into());
            if next_batch > last_ready_batch {
                // Next batch is not ready to be processed yet
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            }
            let handle = tokio::spawn(self.clone().process_batch(next_batch));
            task_handles.push((next_batch, handle));
            next_batch += 1;
        }
    }
}
