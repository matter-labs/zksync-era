use std::fmt;

use async_trait::async_trait;
use multivm::interface::{
    FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionResultAndLogs,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use zksync_types::{vm_trace::Call, Transaction};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::state_keeper::{
    metrics::{ExecutorCommand, EXECUTOR_METRICS},
    types::ExecutionMetricsForCriteria,
};

#[cfg(test)]
mod tests;

pub mod main_executor;

/// Representation of a transaction executed in the virtual machine.
#[derive(Debug, Clone)]
pub(crate) enum TxExecutionResult {
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
#[async_trait]
pub trait BatchExecutor: 'static + Send + Sync + fmt::Debug {
    async fn init_batch(
        &mut self,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<BatchExecutorHandle>;
}

/// A public interface for interaction with the `BatchExecutor`.
/// `BatchExecutorHandle` is stored in the state keeper and is used to invoke or rollback transactions, and also seal
/// the batches.
#[derive(Debug)]
pub struct BatchExecutorHandle {
    handle: JoinHandle<()>,
    commands: mpsc::Sender<Command>,
}

impl BatchExecutorHandle {
    /// Creates a batch executor handle from the provided sender and thread join handle.
    /// Can be used to inject an alternative batch executor implementation.
    #[cfg(test)]
    pub(super) fn from_raw(handle: JoinHandle<()>, commands: mpsc::Sender<Command>) -> Self {
        Self { handle, commands }
    }

    pub(super) async fn execute_tx(&self, tx: Transaction) -> TxExecutionResult {
        let tx_gas_limit = tx.gas_limit().as_u64();

        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::ExecuteTx(Box::new(tx), response_sender))
            .await
            .unwrap();

        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::ExecuteTx]
            .start();
        let res = response_receiver.await.unwrap();
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
        res
    }

    pub(super) async fn start_next_miniblock(&self, miniblock_info: L2BlockEnv) {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::StartNextMiniblock(miniblock_info, response_sender))
            .await
            .unwrap();
        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::StartNextMiniblock]
            .start();
        response_receiver.await.unwrap();
        latency.observe();
    }

    pub(super) async fn rollback_last_tx(&self) {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::RollbackLastTx(response_sender))
            .await
            .unwrap();
        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::RollbackLastTx]
            .start();
        response_receiver.await.unwrap();
        latency.observe();
    }

    pub(super) async fn finish_batch(self) -> FinishedL1Batch {
        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::FinishBatch(response_sender))
            .await
            .unwrap();
        let latency = EXECUTOR_METRICS.batch_executor_command_response_time
            [&ExecutorCommand::FinishBatch]
            .start();
        let finished_batch = response_receiver.await.unwrap();
        self.handle.await.unwrap();
        latency.observe();
        finished_batch
    }
}

#[derive(Debug)]
pub(super) enum Command {
    ExecuteTx(Box<Transaction>, oneshot::Sender<TxExecutionResult>),
    StartNextMiniblock(L2BlockEnv, oneshot::Sender<()>),
    RollbackLastTx(oneshot::Sender<()>),
    FinishBatch(oneshot::Sender<FinishedL1Batch>),
}
