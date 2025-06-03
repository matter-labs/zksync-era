use std::{error::Error as StdError, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use zksync_multivm::interface::{
    executor::BatchExecutor,
    storage::{ReadStorage, StorageView},
    BatchTransactionExecutionResult, FinishedL1Batch, L2BlockEnv,
};
use zksync_types::Transaction;

use super::metrics::{ExecutorCommand, EXECUTOR_METRICS};

#[derive(Debug)]
enum HandleOrError<S> {
    Handle(JoinHandle<anyhow::Result<StorageView<S>>>),
    Err(Arc<dyn StdError + Send + Sync>),
}

impl<S> HandleOrError<S> {
    async fn wait_for_error(&mut self) -> anyhow::Error {
        let err_arc = match self {
            Self::Handle(handle) => {
                let err = match handle.await {
                    Ok(Ok(_)) => anyhow::anyhow!("batch executor unexpectedly stopped"),
                    Ok(Err(err)) => err,
                    Err(err) => anyhow::Error::new(err).context("batch executor panicked"),
                };
                let err: Box<dyn StdError + Send + Sync> = err.into();
                let err: Arc<dyn StdError + Send + Sync> = err.into();
                *self = Self::Err(err.clone());
                err
            }
            Self::Err(err) => err.clone(),
        };
        anyhow::Error::new(err_arc)
    }

    async fn wait(self) -> anyhow::Result<StorageView<S>> {
        match self {
            Self::Handle(handle) => handle.await.context("batch executor panicked")?,
            Self::Err(err_arc) => Err(anyhow::Error::new(err_arc)),
        }
    }
}

/// "Main" [`BatchExecutor`] implementation instantiating a VM in a blocking Tokio thread.
#[derive(Debug)]
pub struct MainBatchExecutor<S> {
    handle: HandleOrError<S>,
    commands: mpsc::Sender<Command>,
}

impl<S: ReadStorage> MainBatchExecutor<S> {
    pub(super) fn new(
        handle: JoinHandle<anyhow::Result<StorageView<S>>>,
        commands: mpsc::Sender<Command>,
    ) -> Self {
        Self {
            handle: HandleOrError::Handle(handle),
            commands,
        }
    }
}

#[async_trait]
impl<S> BatchExecutor<S> for MainBatchExecutor<S>
where
    S: ReadStorage + Send + 'static,
{
    #[tracing::instrument(skip_all)]
    async fn execute_tx(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        let tx_gas_limit = tx.gas_limit().as_u64();

        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::ExecuteTx(Box::new(tx), response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::ExecuteTx]
            .start();
        let res = match response_receiver.await {
            Ok(res) => res,
            Err(_) => return Err(self.handle.wait_for_error().await),
        };
        let elapsed = latency.observe();

        if !res.tx_result.result.is_failed() {
            let gas_used = res.tx_result.statistics.computational_gas_used;
            EXECUTOR_METRICS
                .computational_gas_per_nanosecond
                .observe(gas_used as f64 / elapsed.as_nanos() as f64);
            EXECUTOR_METRICS
                .computational_gas_used
                .observe(gas_used.into());
        } else {
            // The amount of computational gas paid for failed transactions is hard to get
            // but comparing to the gas limit makes sense, since we can burn all gas
            // if some kind of failure is a DDoS vector otherwise.
            EXECUTOR_METRICS
                .failed_tx_gas_limit_per_nanosecond
                .observe(tx_gas_limit as f64 / elapsed.as_nanos() as f64);
            EXECUTOR_METRICS.failed_tx_gas_limit.observe(tx_gas_limit);
        }
        Ok(res)
    }

    #[tracing::instrument(skip_all)]
    async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::RollbackLastTx(response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::RollbackLastTx]
            .start();
        if response_receiver.await.is_err() {
            return Err(self.handle.wait_for_error().await);
        }
        latency.observe();
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()> {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::StartNextL2Block(env, response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::StartNextL2Block]
            .start();
        if response_receiver.await.is_err() {
            return Err(self.handle.wait_for_error().await);
        }
        latency.observe();
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn finish_batch(
        mut self: Box<Self>,
    ) -> anyhow::Result<(FinishedL1Batch, StorageView<S>)> {
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::FinishBatch(response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::FinishBatch]
            .start();
        let finished_batch = match response_receiver.await {
            Ok(batch) => batch,
            Err(_) => return Err(self.handle.wait_for_error().await),
        };
        latency.observe();
        let storage_view = self.handle.wait().await?;
        Ok((finished_batch, storage_view))
    }

    #[tracing::instrument(skip_all)]
    async fn gas_remaining(&mut self) -> anyhow::Result<u32> {
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::GasRemaining(response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::GasRemaining]
            .start();
        let res = match response_receiver.await {
            Ok(res) => res,
            Err(_) => return Err(self.handle.wait_for_error().await),
        };
        latency.observe();
        Ok(res)
    }

    #[tracing::instrument(skip_all)]
    async fn rollback_l2_block(&mut self) -> anyhow::Result<()> {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::RollbackL2Block(response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::RollbackL2Block]
            .start();
        if response_receiver.await.is_err() {
            return Err(self.handle.wait_for_error().await);
        }
        latency.observe();
        Ok(())
    }
}

#[derive(Debug)]
pub(super) enum Command {
    ExecuteTx(
        Box<Transaction>,
        oneshot::Sender<BatchTransactionExecutionResult>,
    ),
    StartNextL2Block(L2BlockEnv, oneshot::Sender<()>),
    RollbackLastTx(oneshot::Sender<()>),
    FinishBatch(oneshot::Sender<FinishedL1Batch>),
    GasRemaining(oneshot::Sender<u32>),
    RollbackL2Block(oneshot::Sender<()>),
}
