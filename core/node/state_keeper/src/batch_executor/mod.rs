use std::{error::Error as StdError, fmt, sync::Arc};

use anyhow::Context as _;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use zksync_multivm::interface::{
    storage::StorageViewCache, CompressedBytecodeInfo, FinishedL1Batch, Halt, L1BatchEnv,
    L2BlockEnv, SystemEnv, VmExecutionResultAndLogs,
};
use zksync_state::OwnedStorage;
use zksync_types::{vm_trace::Call, Transaction};

use crate::{
    metrics::{ExecutorCommand, EXECUTOR_METRICS},
    types::ExecutionMetricsForCriteria,
};

pub mod main_executor;
#[cfg(test)]
mod tests;

/// Representation of a transaction executed in the virtual machine.
#[derive(Debug, Clone)]
pub enum TxExecutionResult {
    /// Successful execution of the tx and the block tip dry run.
    Success {
        tx_result: Box<VmExecutionResultAndLogs>,
        tx_metrics: Box<ExecutionMetricsForCriteria>,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        call_tracer_result: Vec<Call>,
        gas_remaining: u32,
    },
    /// The VM rejected the tx for some reason.
    RejectedByVm { reason: Halt },
    /// Bootloader gas limit is not enough to execute the tx.
    BootloaderOutOfGasForTx,
}

impl TxExecutionResult {
    /// Returns a revert reason if either transaction was rejected or bootloader ran out of gas.
    pub(super) fn err(&self) -> Option<&Halt> {
        match self {
            Self::Success { .. } => None,
            Self::RejectedByVm {
                reason: rejection_reason,
            } => Some(rejection_reason),
            Self::BootloaderOutOfGasForTx => Some(&Halt::BootloaderOutOfGas),
        }
    }
}

/// An abstraction that allows us to create different kinds of batch executors.
/// The only requirement is to return a [`BatchExecutorHandle`], which does its work
/// by communicating with the externally initialized thread.
///
/// This type is generic over the storage type accepted to create the VM instance, mostly for testing purposes.
pub trait BatchExecutor<S = OwnedStorage>: 'static + Send + Sync + fmt::Debug {
    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> BatchExecutorHandle;
}

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
pub struct BatchExecutorHandle {
    handle: HandleOrError,
    commands: mpsc::Sender<Command>,
}

impl BatchExecutorHandle {
    /// Creates a batch executor handle from the provided sender and thread join handle.
    /// Can be used to inject an alternative batch executor implementation.
    #[doc(hidden)]
    pub(super) fn from_raw(
        handle: JoinHandle<anyhow::Result<()>>,
        commands: mpsc::Sender<Command>,
    ) -> Self {
        Self {
            handle: HandleOrError::Handle(handle),
            commands,
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<TxExecutionResult> {
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

        if let TxExecutionResult::Success { tx_metrics, .. } = &res {
            let gas_per_nanosecond = tx_metrics.execution_metrics.computational_gas_used as f64
                / elapsed.as_nanos() as f64;
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
    pub async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()> {
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
    pub async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
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
    pub async fn finish_batch(mut self) -> anyhow::Result<FinishedL1Batch> {
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
        self.handle.wait().await?;
        latency.observe();
        Ok(finished_batch)
    }

    pub async fn finish_batch_with_cache(
        mut self,
    ) -> anyhow::Result<(FinishedL1Batch, StorageViewCache)> {
        let (response_sender, response_receiver) = oneshot::channel();
        let send_failed = self
            .commands
            .send(Command::FinishBatchWithCache(response_sender))
            .await
            .is_err();
        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::FinishBatchWithCache]
            .start();
        let batch_with_cache = match response_receiver.await {
            Ok(batch_with_cache) => batch_with_cache,
            Err(_) => return Err(self.handle.wait_for_error().await),
        };

        self.handle.wait().await?;

        latency.observe();
        Ok(batch_with_cache)
    }
}

#[derive(Debug)]
pub(super) enum Command {
    ExecuteTx(Box<Transaction>, oneshot::Sender<TxExecutionResult>),
    StartNextL2Block(L2BlockEnv, oneshot::Sender<()>),
    RollbackLastTx(oneshot::Sender<()>),
    FinishBatch(oneshot::Sender<FinishedL1Batch>),
    FinishBatchWithCache(oneshot::Sender<(FinishedL1Batch, StorageViewCache)>),
}
