use anyhow::{anyhow, Result};
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::time::Sleep;
use futures::StreamExt;
use futures_core::{Stream, FusedStream};
use zk_ee::system::errors::InternalError;
use zk_os_forward_system::run::{BatchContext, BatchOutput, InvalidTransaction};
use zksync_types::Transaction;
use zksync_zkos_vm_runner::zkos_conversions::tx_abi_encode;
use crate::BLOCK_TIME_MS;
use crate::execution::vm_wrapper::VmWrapper;
use crate::mempool::Mempool;
use crate::model::{BlockCommand, ReplayRecord};
use crate::storage::StateHandle;

// What to do when VM returns an InvalidTransaction error.
#[derive(Clone, Copy, Debug)]
enum InvalidTxPolicy {
    /// Invalid tx is skipped in block and discarded from mempool. Used when building a block.
    RejectAndContinue,
    /// Bubble the error up and abort the whole block. Used when replaying a block (ReplayLog / Replica / EN)
    Abort,
}

#[derive(Clone, Copy, Debug)]
enum SealPolicy {
    /// Seal non-empty blocks after deadline. Used when building a block.
    Deadline(Duration),
    /// Seal when all txs from tx source are executed. Used when replaying a block (ReplayLog / Replica / EN)
    Exhausted,
}

/// A stream of transactions that can be `await`-ed item-by-item.
type TxStream = Pin<Box<dyn Stream<Item=Transaction> + Send>>;

/// Build everything the VM runner needs for this command:
///   – the context (`BatchContext`)
///   – the stream (`TxStream`)
///   – the seal and invalid-tx policies
///
/// The `mempool` parameter is *used only* by `Produce`.
fn command_into_parts(
    block_command: BlockCommand,
    mempool: Mempool,
) -> (
    BatchContext,
    TxStream,
    SealPolicy,
    InvalidTxPolicy,
) {
    match block_command {
        BlockCommand::Produce(ctx) => (
            ctx,
            Box::pin(MempoolStream::new(mempool.clone())) as TxStream,
            SealPolicy::Deadline(Duration::from_millis(BLOCK_TIME_MS)),
            InvalidTxPolicy::RejectAndContinue,
        ),
        BlockCommand::Replay(replay) => (
            replay.context,
            Box::pin(futures::stream::iter(replay.transactions)) as TxStream,
            SealPolicy::Exhausted,
            InvalidTxPolicy::Abort,
        ),
    }
}

pub async fn execute_block(
    cmd: BlockCommand,
    mempool: Mempool,
    state: StateHandle,
) -> Result<(BatchOutput, ReplayRecord)> {
    let (ctx, stream, seal, invalid) = command_into_parts(cmd, mempool);
    execute_block_inner(ctx, state, stream, seal, invalid).await
}


async fn execute_block_inner(
    ctx:          BatchContext,
    state:        StateHandle,
    mut txs:      TxStream,
    seal_policy:  SealPolicy,
    fail_policy:  InvalidTxPolicy,
) -> Result<(BatchOutput, ReplayRecord)> {
    tracing::info!(block = ctx.block_number, "start");

    let state_view = state.view_at(ctx.block_number)?;
    let mut runner = VmWrapper::new(ctx.clone(), state_view);
    let mut executed: Vec<Transaction> = Vec::new();

    /* -------- 1. pre-fetch first tx ---------------------------------- */
    let mut next_tx_opt = txs.next().await;
    if next_tx_opt.is_none() {
        return Err(anyhow!("empty replay for block {}", ctx.block_number));
    }

    let mut deadline: Option<Pin<Box<Sleep>>> = match seal_policy {
        SealPolicy::Deadline(d) => Some(Box::pin(tokio::time::sleep(d))),
        SealPolicy::Exhausted   => None,
    };

    /* -------- 2. main loop ------------------------------------------- */
    loop {
        // future that resolves to Option<Transaction>
        let tx_future = async {
            if let Some(tx) = next_tx_opt.take() {
                Some(tx)                   // pre-fetched one
            } else {
                txs.next().await           // poll stream
            }
        };

        tokio::select! {
            // deadline branch
            _ = async { if let Some(s) = &mut deadline { s.as_mut().await } },
              if deadline.is_some() => {
                tracing::info!(block = ctx.block_number, "deadline reached; sealing");
                break;
            }

            // tx streaming branch
            maybe_tx = tx_future => {
                match maybe_tx {
                    Some(tx) => {
                        // clone :(
                        match runner.execute_next_tx(tx_abi_encode(tx.clone())).await {
                            Ok(_)  => executed.push(tx),
                            Err(e) => match fail_policy {
                                InvalidTxPolicy::RejectAndContinue => {
                                    tracing::warn!(block = ctx.block_number, ?e, "invalid; skipping");
                                }
                                InvalidTxPolicy::Abort => return Err(anyhow!("invalid tx: {e:?}")),
                            },
                        }
                    }
                    None => break,        // stream finished → seal
                }
            }
        }
    }

    /* -------- 3. seal & return -------------------------------------- */
    let output = runner
        .seal_batch()
        .await
        .map_err(|e| anyhow!("VM seal failed: {e:?}"))?;

    Ok((output, ReplayRecord { context: ctx, transactions: executed }))
}



/// ----- Minimal adapter turning a mempool into a `Stream` ------------------

struct MempoolStream {
    mempool: Mempool,
}

impl MempoolStream {
    fn new(mempool: Mempool) -> Self { Self { mempool } }
}

impl Stream for MempoolStream {
    type Item = Transaction;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // *Request*: make this function async on `Mempool` so we can just call
        //  `self.mempool.next_tx().await`.  For now we fall back to the old
        //  busy-wait API behind `tokio::task::block_in_place` to avoid
        //  blocking the reactor.
        match self.mempool.get_next() {
            Some(tx) => Poll::Ready(Some(tx)),
            None => {
                // re-arm the waker every X ms (5 ms from the original code)
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    waker.wake();
                });
                Poll::Pending
            }
        }
    }
}
