use std::{error::Error as StdError, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use zksync_multivm::interface::{
    executor::{BatchExecutorHandle, InspectStorage, InspectStorageFn},
    storage::ReadStorage,
    BatchTransactionExecutionResult, FinishedL1Batch, L2BlockEnv,
};
use zksync_types::Transaction;

use super::metrics::{ExecutorCommand, EXECUTOR_METRICS};

#[derive(Debug)]
enum HandleOrError {
    Handle(JoinHandle<anyhow::Result<()>>),
    Err(Arc<dyn StdError + Send + Sync>),
}

impl HandleOrError {
    async fn wait_for_error(&mut self) -> anyhow::Error {
        let err_arc = match self {
            Self::Handle(handle) => {
                let err = match handle.await {
                    Ok(Ok(())) => anyhow::anyhow!("batch executor unexpectedly stopped"),
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

    #[allow(dead_code)] // FIXME: is it OK to not wait for handle?
    async fn wait(self) -> anyhow::Result<()> {
        match self {
            Self::Handle(handle) => handle.await.context("batch executor panicked")?,
            Self::Err(err_arc) => Err(anyhow::Error::new(err_arc)),
        }
    }
}

/// A public interface for interaction with the `BatchExecutor`.
/// `BatchExecutorHandle` is stored in the state keeper and is used to invoke or rollback transactions, and also seal
/// the batches.
#[derive(Debug)]
pub struct MainBatchExecutorHandle<S> {
    handle: HandleOrError,
    commands: mpsc::Sender<Command<S>>,
}

impl<S: ReadStorage> MainBatchExecutorHandle<S> {
    pub(super) fn new(
        handle: JoinHandle<anyhow::Result<()>>,
        commands: mpsc::Sender<Command<S>>,
    ) -> Self {
        Self {
            handle: HandleOrError::Handle(handle),
            commands,
        }
    }
}

#[async_trait]
impl<S> InspectStorage<S> for MainBatchExecutorHandle<S>
where
    S: ReadStorage + 'static,
{
    #[tracing::instrument(skip_all)]
    async fn inspect_storage(&mut self, f: InspectStorageFn<S>) -> anyhow::Result<()> {
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::InspectStorage(f, response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::InspectStorage]
            .start();
        if response_receiver.await.is_err() {
            return Err(self.handle.wait_for_error().await);
        }
        latency.observe();
        Ok(())
    }
}

#[async_trait]
impl<S> BatchExecutorHandle<S> for MainBatchExecutorHandle<S>
where
    S: ReadStorage + 'static,
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
            let gas_per_nanosecond =
                res.tx_result.statistics.computational_gas_used as f64 / elapsed.as_nanos() as f64;
            EXECUTOR_METRICS
                .computational_gas_per_nanosecond
                .observe(gas_per_nanosecond);
        } else {
            // The amount of computational gas paid for failed transactions is hard to get
            // but comparing to the gas limit makes sense, since we can burn all gas
            // if some kind of failure is a DDoS vector otherwise.
            EXECUTOR_METRICS
                .failed_tx_gas_limit_per_nanosecond
                .observe(tx_gas_limit as f64 / elapsed.as_nanos() as f64);
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
    ) -> anyhow::Result<(FinishedL1Batch, Box<dyn InspectStorage<S>>)> {
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
        Ok((finished_batch, self))
    }
}

pub(super) enum Command<S> {
    ExecuteTx(
        Box<Transaction>,
        oneshot::Sender<BatchTransactionExecutionResult>,
    ),
    StartNextL2Block(L2BlockEnv, oneshot::Sender<()>),
    RollbackLastTx(oneshot::Sender<()>),
    FinishBatch(oneshot::Sender<FinishedL1Batch>),
    InspectStorage(InspectStorageFn<S>, oneshot::Sender<()>),
}
