use std::{sync::Arc, time::Duration};

use multivm::interface::L2BlockEnv;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_state::ReadStorageFactory;
use zksync_state_keeper::{
    BatchExecutor, ExecutionMetricsForCriteria, L2BlockParams, TxExecutionResult, UpdatesManager,
};

use crate::{storage::StorageLoader, OutputHandlerFactory, VmRunnerIo};

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
    storage: Arc<dyn ReadStorageFactory>,
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
        // Normally `storage` and `loader` would be the same object, but it is awkward to deal with
        // them as such due to Rust's limitations on upcasting coercion. See
        // https://github.com/rust-lang/rust/issues/65991.
        storage: Arc<dyn ReadStorageFactory>,
        loader: Arc<dyn StorageLoader>,
        output_handler_factory: Box<dyn OutputHandlerFactory>,
        batch_processor: Box<dyn BatchExecutor>,
    ) -> Self {
        Self {
            pool,
            io,
            storage,
            loader,
            output_handler_factory,
            batch_processor,
        }
    }

    /// Consumes VM runner to execute a loop that continuously pulls data from [`VmRunnerIo`] and
    /// processes it.
    pub async fn run(mut self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(50);

        let mut next_batch = self
            .io
            .latest_processed_batch(&mut self.pool.connection().await?)
            .await?
            + 1;
        loop {
            let last_ready_batch = self
                .io
                .last_ready_to_be_loaded_batch(&mut self.pool.connection().await?)
                .await?;
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
            let mut updates_manager =
                UpdatesManager::new(&batch_data.l1_batch_env, &batch_data.system_env);
            let Some(handler) = self
                .batch_processor
                .init_batch(
                    self.storage.clone(),
                    batch_data.l1_batch_env,
                    batch_data.system_env,
                    stop_receiver,
                )
                .await
            else {
                tracing::info!("VM runner was interrupted");
                break;
            };
            let mut output_handler = self
                .output_handler_factory
                .create_handler(next_batch)
                .await?;

            tokio::task::spawn(async move {
                for (i, l2_block) in batch_data.l2_blocks.into_iter().enumerate() {
                    if i > 0 {
                        // First L2 block in every batch is already preloaded
                        updates_manager.push_l2_block(L2BlockParams {
                            timestamp: l2_block.timestamp,
                            virtual_blocks: l2_block.virtual_blocks,
                        });
                        handler
                            .start_next_l2_block(L2BlockEnv::from_l2_block_data(&l2_block))
                            .await;
                    }
                    for tx in l2_block.txs {
                        let exec_result = handler.execute_tx(tx.clone()).await;
                        let TxExecutionResult::Success {
                            tx_result,
                            tx_metrics,
                            call_tracer_result,
                            compressed_bytecodes,
                            ..
                        } = exec_result
                        else {
                            panic!("Unexpected non-successful transaction");
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
                        .expect("VM runner failed to handle L2 block");
                }
                handler.finish_batch().await;
                output_handler
                    .handle_l1_batch(Arc::new(updates_manager))
                    .await
                    .expect("VM runner failed to handle L1 batch");
            });

            next_batch += 1;
        }

        Ok(())
    }
}
