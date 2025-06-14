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
use crate::execution::metrics;
use crate::execution::metrics::{EXECUTION_METRICS};
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
            Box::pin(mempool),
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
    let metrics_label = match cmd {
        BlockCommand::Produce(_) => "produce",
        BlockCommand::Replay(_) => "replay",
    };
    let (ctx, stream, seal, invalid) = command_into_parts(cmd, mempool);
    execute_block_inner(ctx, state, stream, seal, invalid, metrics_label).await
}

async fn execute_block_inner(
    ctx: BatchContext,
    state: StateHandle,
    mut txs: TxStream,
    seal_policy: SealPolicy,
    fail_policy: InvalidTxPolicy,
    metrics_label: &'static str,
) -> Result<(BatchOutput, ReplayRecord)> {
    tracing::info!(block = ctx.block_number, "start");

    /* ---------- VM & state ----------------------------------------- */
    let state_view = state.view_at(ctx.block_number)?;
    let mut runner = VmWrapper::new(ctx.clone(), state_view);
    let mut executed = Vec::<Transaction>::new();

    /* ---------- deadline config ------------------------------------ */
    let deadline_dur = match seal_policy {
        SealPolicy::Deadline(d) => Some(d),
        SealPolicy::Exhausted => None,
    };
    let mut deadline: Option<Pin<Box<Sleep>>> = None;   // will arm after 1st success

    /* ---------- main loop ------------------------------------------ */
    loop {
        let mut wait_for_tx_latency = EXECUTION_METRICS.block_execution_stages[&"wait_for_tx"].start();
        tokio::select! {
            /* -------- deadline branch ------------------------------ */
            _ = async {
                    if let Some(d) = &mut deadline {
                        d.as_mut().await
                    }
                },
                if deadline.is_some()
            => {
                tracing::info!(block = ctx.block_number,
                               txs = executed.len(),
                               "deadline reached → sealing");
                break;                                     // leave the loop ⇒ seal
            }

            /* -------- stream branch ------------------------------- */
            maybe_tx = txs.next() => {
                match maybe_tx {
                    /* ----- got a transaction ---------------------- */
                    Some(tx) => {
                        wait_for_tx_latency.observe();
                        let mut latency = EXECUTION_METRICS.block_execution_stages[&"execute"].start();
                        match runner.execute_next_tx(tx_abi_encode(tx.clone())).await {
                            Ok(res) => {
                                // tracing::info!(block = ctx.block_number,
                                //                tx = ?tx.hash(),
                                //                 res = ?res,
                                //                "tx executed");
                                latency.observe();
                                EXECUTION_METRICS.executed_transactions[&metrics_label.as_ref()].inc();

                                executed.push(tx);

                                // arm the timer once, after first successful tx
                                if deadline.is_none() {
                                    if let Some(dur) = deadline_dur {
                                        deadline = Some(Box::pin(tokio::time::sleep(dur)));
                                    }
                                }
                            }
                            Err(e) => match fail_policy {
                                InvalidTxPolicy::RejectAndContinue => {
                                    tracing::warn!(block = ctx.block_number, ?e,
                                                   "invalid tx → skipped");
                                }
                                InvalidTxPolicy::Abort => {
                                    return Err(anyhow!("invalid tx: {e:?}"));
                                }
                            }
                        }
                        wait_for_tx_latency = EXECUTION_METRICS.block_execution_stages[&"wait_for_tx"].start();
                    }

                    /* ----- stream ended --------------------------- */
                    None => {
                        if executed.is_empty() && matches!(seal_policy, SealPolicy::Exhausted)
                        {
                            // Replay path requires at least one tx.
                            return Err(anyhow!(
                                "empty replay for block {}",
                                ctx.block_number
                            ));
                        }

                        tracing::info!(block = ctx.block_number,
                                       txs = executed.len(),
                                       "stream exhausted → sealing");
                        break;
                    }
                }
            }
        }
    }
    let latency = EXECUTION_METRICS.block_execution_stages[&"seal"].start();

    /* ---------- seal & return ------------------------------------- */
    let output = runner
        .seal_batch()
        .await
        .map_err(|e| anyhow!("VM seal failed: {e:?}"))?;
    latency.observe();
    Ok((output,
        ReplayRecord {
            context: ctx,
            transactions: executed,
        }))
}