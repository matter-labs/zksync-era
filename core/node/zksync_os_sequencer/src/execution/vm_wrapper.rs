// batch_runner.rs

use std::{
    sync::Arc,
    error::Error as StdError,
};
use tokio::{
    sync::mpsc::{channel, Sender, Receiver},
    task::{spawn_blocking, JoinHandle},
};
use zk_os_forward_system::run::{
    run_batch, BatchContext, BatchOutput, NextTxResponse,
    InvalidTransaction, TxSource, TxResultCallback,
    PreimageSource, ReadStorageTree,
};
use zk_ee::system::errors::InternalError;
use zk_os_forward_system::run::result_keeper::TxProcessingOutputOwned;

/// Internal handle for the spawned `run_batch` task.
enum Handle {
    /// The VM runner is still active.
    Running(JoinHandle<Result<BatchOutput, InternalError>>),
    /// The VM runner failed.
    Failed(InternalError),
}

/// A one‐by‐one driver around `run_batch`, letting you push raw tx bytes
/// and then seal when you choose.
///
pub struct VmWrapper {
    handle: Handle,
    tx_sender: Sender<NextTxResponse>,
    tx_result_receiver: Receiver<Result<TxProcessingOutputOwned, InvalidTransaction>>,
}

impl VmWrapper {
    /// Spawn the VM runner in a blocking task.
    pub fn new(
        context: BatchContext,
        state_view: impl ReadStorageTree + PreimageSource + Send + Clone + 'static,
    ) -> Self {
        // Channel for sending NextTxResponse (Tx bytes or SealBatch).
        let (tx_sender, tx_receiver) = channel(1);
        // Channel for receiving per‐tx execution results.
        let (res_sender, res_receiver) = channel(1);

        // Wrap the channels in the traits run_batch expects:
        let tx_source = ChannelTxSource::new(tx_receiver);
        let tx_callback = ChannelTxResultCallback::new(res_sender);

        // Spawn the blocking run_batch(...) call.
        let join_handle = spawn_blocking(move || {
            run_batch(
                context,
                state_view.clone(),
                state_view,
                tx_source,
                tx_callback,
            )
        });

        Self {
            handle: Handle::Running(join_handle),
            tx_sender,
            tx_result_receiver: res_receiver,
        }
    }

    /// Send one transaction to the VM and await its execution result.
    ///
    /// Returns Ok(output) on success, or Err(InvalidTransaction) if the VM
    /// rejected it. In case of an error, you can then call `seal_batch()`
    /// to finish the block.
    pub async fn execute_next_tx(
        &mut self,
        raw_tx: Vec<u8>,
    ) -> Result<TxProcessingOutputOwned, InvalidTransaction> {
        // Send the next‐tx request.
        // If this fails, the runner has already shut down.
        if self.tx_sender.send(NextTxResponse::Tx(raw_tx)).await.is_err() {
            panic!("BatchRunner: tx_source channel closed unexpectedly");
        }
        // Await the VM's callback.
        match self.tx_result_receiver.recv().await {
            Some(Ok(output)) => Ok(output),
            Some(Err(invalid)) => Err(invalid),
            None => {
                // No more tx results means run_batch has exited.
                panic!("BatchRunner: tx_result channel closed unexpectedly");
            }
        }
    }

    /// Tell the VM to seal the batch and return the final `BatchOutput`.
    pub async fn seal_batch(mut self) -> Result<BatchOutput, InternalError> {
        // Request batch seal.
        let _ = self.tx_sender.send(NextTxResponse::SealBatch).await;
        // Await the blocking task's result.
        match self.handle {
            Handle::Running(handle) => handle
                .await
                .map_err(|e| InternalError("runner panicked"))?
            ,
            Handle::Failed(err) => return Err(err),
        }
    }
}

/// A `TxSource` that drives `run_batch` from a `tokio::sync::mpsc::Receiver`.
struct ChannelTxSource {
    receiver: Receiver<NextTxResponse>,
}

impl ChannelTxSource {
    fn new(receiver: Receiver<NextTxResponse>) -> Self {
        Self { receiver }
    }
}

impl TxSource for ChannelTxSource {
    fn get_next_tx(&mut self) -> NextTxResponse {
        // Block until we get a request.
        // If the sender is dropped, default to sealing.
        self.receiver
            .blocking_recv()
            .unwrap_or(NextTxResponse::SealBatch)
    }
}

/// A `TxResultCallback` that forwards each result into a `tokio::sync::mpsc::Sender`.
struct ChannelTxResultCallback {
    sender: Sender<Result<TxProcessingOutputOwned, InvalidTransaction>>,
}

impl ChannelTxResultCallback {
    fn new(sender: Sender<Result<TxProcessingOutputOwned, InvalidTransaction>>) -> Self {
        Self { sender }
    }
}

impl TxResultCallback for ChannelTxResultCallback {
    fn tx_executed(
        &mut self,
        tx_execution_result: Result<TxProcessingOutputOwned, InvalidTransaction>,
    ) {
        // Fire-and-forget the result into the channel.
        // We're on the blocking thread, so use blocking_send.
        let _ = self.sender.blocking_send(tx_execution_result);
    }
}
