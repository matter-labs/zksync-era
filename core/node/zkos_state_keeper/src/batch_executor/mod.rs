use std::sync::Arc;

use anyhow::Context;
use serde::de::StdError;
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::{spawn_blocking, JoinHandle},
};
use zk_ee::system::errors::InternalError;
use zk_os_forward_system::run::{
    result_keeper::TxProcessingOutputOwned, run_batch, BatchContext, BatchOutput,
    InvalidTransaction, NextTxResponse, StorageCommitment, TxResultCallback, TxSource,
};
use zksync_state::ArcOwnedStorage;
use zksync_types::Transaction;
use zksync_zkos_vm_runner::zkos_conversions::tx_abi_encode;

pub type BatchExecutorTxResult = Result<TxProcessingOutputOwned, InvalidTransaction>;

#[derive(Debug)]
enum HandleOrError {
    Handle(JoinHandle<Result<BatchOutput, InternalError>>),
    Err(Arc<dyn StdError + Send + Sync>),
}

impl HandleOrError {
    async fn wait_for_error(&mut self) -> anyhow::Error {
        let err_arc = match self {
            Self::Handle(handle) => {
                let err = match handle.await {
                    Ok(Ok(_)) => anyhow::anyhow!("batch executor unexpectedly stopped"),
                    Ok(Err(err)) => anyhow::anyhow!("{}", err.0).context("zk_ee internal error"),
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

    async fn wait(self) -> anyhow::Result<BatchOutput> {
        match self {
            Self::Handle(handle) => handle
                .await
                .context("batch executor panicked")?
                .map_err(|err| anyhow::anyhow!("{}", err.0).context("zk_ee internal error")),
            Self::Err(err_arc) => Err(anyhow::Error::new(err_arc)),
        }
    }
}

pub struct MainBatchExecutor {
    handle: HandleOrError,
    tx_sender: Sender<NextTxResponse>,
    tx_result_receiver: Receiver<Result<TxProcessingOutputOwned, InvalidTransaction>>,
}

impl MainBatchExecutor {
    pub fn new(context: BatchContext, storage: ArcOwnedStorage) -> Self {
        let (tx_sender, tx_receiver) = tokio::sync::mpsc::channel(1);
        let (tx_result_sender, tx_result_receiver) = tokio::sync::mpsc::channel(1);
        let tx_source = OnlineTxSource::new(tx_receiver);
        let tx_callback = ChannelTxResultCallback::new(tx_result_sender);

        let handle = spawn_blocking(move || {
            run_batch(
                context,
                storage.clone(),
                storage,
                tx_source,
                tx_callback,
            )
        });
        Self {
            handle: HandleOrError::Handle(handle),
            tx_sender,
            tx_result_receiver,
        }
    }

    pub async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<BatchExecutorTxResult> {
        let encoded_tx = tx_abi_encode(tx.clone());

        let send_failed = self
            .tx_sender
            .send(NextTxResponse::Tx(encoded_tx))
            .await
            .is_err();

        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        match self.tx_result_receiver.recv().await {
            Some(res) => Ok(res),
            None => Err(self.handle.wait_for_error().await),
        }
    }

    pub async fn finish_batch(mut self) -> anyhow::Result<BatchOutput> {
        let send_failed = self
            .tx_sender
            .send(NextTxResponse::SealBatch)
            .await
            .is_err();

        if send_failed {
            return Err(self.handle.wait_for_error().await);
        }

        self.handle.wait().await
    }
}

#[derive(Debug)]
struct OnlineTxSource {
    receiver: Receiver<NextTxResponse>,
}

impl OnlineTxSource {
    pub fn new(receiver: Receiver<NextTxResponse>) -> Self {
        Self { receiver }
    }
}

impl TxSource for OnlineTxSource {
    fn get_next_tx(&mut self) -> NextTxResponse {
        self.receiver.blocking_recv().unwrap_or_else(|| {
            // Sender shouldn't be dropped i.e. we always try to finish block execution properly
            // on shutdown request. This looks reasonable given block will take short period of time
            // (most likely 1 second), and if there are no transactions we can still close an empty block
            // without persisting it.
            panic!("next tx sender was dropped without yielding `SealBatch`")
        })
    }
}

#[derive(Debug)]
struct ChannelTxResultCallback {
    sender: Sender<Result<TxProcessingOutputOwned, InvalidTransaction>>,
}

impl ChannelTxResultCallback {
    pub fn new(sender: Sender<Result<TxProcessingOutputOwned, InvalidTransaction>>) -> Self {
        Self { sender }
    }
}

impl TxResultCallback for ChannelTxResultCallback {
    fn tx_executed(
        &mut self,
        tx_execution_result: Result<TxProcessingOutputOwned, InvalidTransaction>,
    ) {
        self.sender
            .blocking_send(tx_execution_result)
            .expect("Tx execution result receiver was dropped");
    }
}
