use crate::storage::StorageLoader;
use crate::{OutputHandlerFactory, VmRunnerIo};
use multivm::interface::L2BlockEnv;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use zksync_core::state_keeper::{
    BatchExecutor, ExecutionMetricsForCriteria, L2BlockParams, TxExecutionResult, UpdatesManager,
};
use zksync_dal::{ConnectionPool, Core};

#[derive(Debug)]
pub struct VmRunner {
    pool: ConnectionPool<Core>,
    io: Box<dyn VmRunnerIo>,
    storage: Arc<dyn StorageLoader>,
    output_handler_factory: Box<dyn OutputHandlerFactory>,
    batch_processor: Box<dyn BatchExecutor>,
}

impl VmRunner {
    pub fn new(
        pool: ConnectionPool<Core>,
        io: Box<dyn VmRunnerIo>,
        storage: Arc<dyn StorageLoader>,
        output_handler_factory: Box<dyn OutputHandlerFactory>,
        batch_processor: Box<dyn BatchExecutor>,
    ) -> Self {
        Self {
            pool,
            io,
            storage,
            output_handler_factory,
            batch_processor,
        }
    }

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
            let Some(batch_data) = self.storage.load_batch(next_batch).await? else {
                // Next batch has not been loaded yet
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            };
            let mut updates_manager =
                UpdatesManager::new(&batch_data.l1_batch_env, &batch_data.system_env);
            let Some(handler) = self
                .batch_processor
                .init_batch(
                    batch_data.l1_batch_env,
                    batch_data.system_env,
                    stop_receiver,
                )
                .await
            else {
                tracing::info!("Interrupted");
                break;
            };
            let mut output_handler = self
                .output_handler_factory
                .create_handler(next_batch)
                .await?;

            tokio::task::spawn(async move {
                for l2_block in batch_data.l2_blocks {
                    updates_manager.push_l2_block(L2BlockParams {
                        timestamp: l2_block.timestamp,
                        virtual_blocks: l2_block.virtual_blocks,
                    });
                    handler
                        .start_next_l2_block(L2BlockEnv::from_l2_block_data(&l2_block))
                        .await;
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
                            tracing::error!("Unexpected non-successful transaction");
                            break;
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
