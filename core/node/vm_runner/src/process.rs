use std::{sync::Arc, time::Duration};

use anyhow::Context;
use tokio::{sync::watch, task::JoinHandle};
use zksync_dal::{ConnectionPool, Core};
use zksync_multivm::interface::L2BlockEnv;
use zksync_state_keeper::{
    BatchExecutor, BatchExecutorHandle, ExecutionMetricsForCriteria, L2BlockParams,
    StateKeeperOutputHandler, TxExecutionResult, UpdatesManager,
};
use zksync_types::{block::L2BlockExecutionData, L1BatchNumber};

use crate::{metrics::METRICS, storage::StorageLoader, OutputHandlerFactory, VmRunnerIo};

/// VM runner represents a logic layer of L1 batch / L2 block processing flow akin to that of state
/// keeper. The difference is that VM runner is designed to be run on batches/blocks that have
/// already been processed by state keeper but still require some extra handling as regulated by
/// [`OutputHandlerFactory`].
///
/// It's responsible for taking unprocessed data from the [`VmRunnerIo`], feeding it into
/// [`BatchExecutor`] and calling [`OutputHandlerFactory`] on the result of the execution (batch
/// execution state in the [`UpdatesManager`]).
///
/// You can think of VM runner as a concurrent processor of a continuous stream of newly committed
/// batches/blocks.
#[derive(Debug)]
pub struct VmRunner {
    pool: ConnectionPool<Core>,
    io: Box<dyn VmRunnerIo>,
    loader: Arc<dyn StorageLoader>,
    output_handler_factory: Box<dyn OutputHandlerFactory>,
    batch_processor: Box<dyn BatchExecutor>,
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
        io: Box<dyn VmRunnerIo>,
        loader: Arc<dyn StorageLoader>,
        output_handler_factory: Box<dyn OutputHandlerFactory>,
        batch_processor: Box<dyn BatchExecutor>,
    ) -> Self {
        Self {
            pool,
            io,
            loader,
            output_handler_factory,
            batch_processor,
        }
    }

    async fn process_batch(
        mut batch_executor: BatchExecutorHandle,
        l2_blocks: Vec<L2BlockExecutionData>,
        mut updates_manager: UpdatesManager,
        mut output_handler: Box<dyn StateKeeperOutputHandler>,
    ) -> anyhow::Result<()> {
        let latency = METRICS.run_vm_time.start();
        for (i, l2_block) in l2_blocks.into_iter().enumerate() {
            if i > 0 {
                // First L2 block in every batch is already preloaded
                updates_manager.push_l2_block(L2BlockParams {
                    timestamp: l2_block.timestamp,
                    virtual_blocks: l2_block.virtual_blocks,
                });
                let block_env = L2BlockEnv::from_l2_block_data(&l2_block);
                batch_executor
                    .start_next_l2_block(block_env)
                    .await
                    .with_context(|| {
                        format!("failed starting L2 block with {block_env:?} in batch executor")
                    })?;
            }
            for tx in l2_block.txs {
                let exec_result = batch_executor
                    .execute_tx(tx.clone())
                    .await
                    .with_context(|| format!("failed executing transaction {:?}", tx.hash()))?;
                let TxExecutionResult::Success {
                    tx_result,
                    tx_metrics,
                    call_tracer_result,
                    compressed_bytecodes,
                    ..
                } = exec_result
                else {
                    anyhow::bail!("Unexpected non-successful transaction");
                };
                let ExecutionMetricsForCriteria {
                    l1_gas: tx_l1_gas_this_tx,
                    execution_metrics: tx_execution_metrics,
                } = *tx_metrics;
                updates_manager.extend_from_executed_transaction(
                    tx,
                    *tx_result,
                    compressed_bytecodes,
                    tx_l1_gas_this_tx,
                    tx_execution_metrics,
                    call_tracer_result,
                );
            }
            output_handler
                .handle_l2_block(&updates_manager)
                .await
                .context("VM runner failed to handle L2 block")?;
        }
        let finished_batch = batch_executor
            .finish_batch()
            .await
            .context("failed finishing L1 batch in executor")?;
        updates_manager.finish_batch(finished_batch);
        latency.observe();
        output_handler
            .handle_l1_batch(Arc::new(updates_manager))
            .await
            .context("VM runner failed to handle L1 batch")?;
        Ok(())
    }

    /// Consumes VM runner to execute a loop that continuously pulls data from [`VmRunnerIo`] and
    /// processes it.
    pub async fn run(mut self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(50);

        // Join handles for asynchronous tasks that are being run in the background
        let mut task_handles: Vec<(L1BatchNumber, JoinHandle<anyhow::Result<()>>)> = Vec::new();
        let mut next_batch = self
            .io
            .latest_processed_batch(&mut self.pool.connection().await?)
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
                .last_ready_to_be_loaded_batch(&mut self.pool.connection().await?)
                .await?;
            METRICS.last_ready_batch.set(last_ready_batch.0.into());
            if next_batch > last_ready_batch {
                // Next batch is not ready to be processed yet
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            }
            let Some(batch_data) = self.loader.load_batch(next_batch).await? else {
                // Next batch has not been loaded yet
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            };
            let updates_manager =
                UpdatesManager::new(&batch_data.l1_batch_env, &batch_data.system_env);
            let Some(batch_executor) = self
                .batch_processor
                .init_batch(
                    self.loader.clone().upcast(),
                    batch_data.l1_batch_env,
                    batch_data.system_env,
                    stop_receiver,
                )
                .await
            else {
                tracing::info!("VM runner was interrupted");
                break;
            };
            let output_handler = self
                .output_handler_factory
                .create_handler(next_batch)
                .await?;

            let handle = tokio::task::spawn(Self::process_batch(
                batch_executor,
                batch_data.l2_blocks,
                updates_manager,
                output_handler,
            ));
            task_handles.push((next_batch, handle));

            next_batch += 1;
        }

        Ok(())
    }
}
