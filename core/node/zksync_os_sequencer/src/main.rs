// sequencer_stream.rs

mod storage;
mod model;
mod block_context_provider;
mod api;
mod mempool;
mod util;
mod execution;
mod tx_conversions;

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use futures::stream::{self, Stream, StreamExt, Fuse, Chain, TryStream, TryStreamExt};
use futures::future::FutureExt;
use futures::pin_mut;
use futures::stream::BoxStream;
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::{BatchContext, BatchOutput};
use zksync_types::{Address, address_to_h256, H256};
use crate::api::run_jsonrpsee_server;
use crate::block_context_provider::BlockContextProvider;
use crate::execution::block_executor::{execute_block};
use crate::mempool::Mempool;
use crate::model::{BlockCommand, TransactionSource};
use crate::storage::block_replay_wal::{BlockReplayWAL};
use crate::storage::in_memory_state::{InMemoryStorage};
use crate::storage::StateHandle;
use anyhow::{Result, Context};
use itertools::Itertools;
// Terms:
// * BlockReplayData     - minimal info to (re)apply the block.
//
// * `Canonize` block operation   - after block is processed in the VM, we want to expose it in API asap.
//                         But we can only do that once it's durable - that is, it will not change after node crash/restart.
//                         Canonization is the process of making a block durable.
//                         For the centralized sequencer we persist a WAL of `BlockReplayData`s
//                         For leader rotation we only consider block Canonized when it's accepted by the network (we have quorum)

// Node state consists of three things:

// * State (storage logs + factory deps)
// * BlockReceipts (events + pubdata + other heavy info)
// * WAL of BlockReplayData (only in centralized case)
//
// Note that we only ever persist canonized blocks

const BLOCK_REPLAY_WAL_PATH: &str = "../chains/era/db/main/block_replay_wal";
const CHAIN_ID: u64 = 270;

// Maximum number of per-block information stored in memory - and thus returned from API.
// Older blocks are discarded (or, in case of state diffs, compacted)
const BLOCKS_TO_RETAIN: usize = 1000;

const JSON_RPC_ADDR: &str = "127.0.0.1:3050";

const BLOCK_TIME_MS: u64 = 150;

const MAX_TX_SIZE: usize = 100000;

#[tokio::main]
pub async fn main() {
    tracing_subscriber::fmt()
        // .pretty()
        .init();

    tracing::warn!("TODO: Current Implementation doesn't persist Block Recipies or State - so we replay all blocks starting from genesis");

    let block_replay_wal = BlockReplayWAL::new(Path::new(BLOCK_REPLAY_WAL_PATH))
        .expect("Failed to open BlockReplayWAL");

    let state_handle = StateHandle::empty();
    let mempool = Mempool::new();

    tokio::select! {
        // ── JSON-RPC task ────────────────────────────────────────────────
        res = run_jsonrpsee_server(state_handle.clone(), mempool.clone()) => {
            match res {
                Ok(_)  => tracing::warn!("JSON-RPC server unexpectedly exited"),
                Err(e) => tracing::error!("JSON-RPC server failed: {e:#}"),
            }
        }

        // ── Sequencer task ───────────────────────────────────────────────
        res = run_sequencer(block_replay_wal, state_handle, mempool) => {
            match res {
                Ok(_)  => tracing::warn!("Sequencer server unexpectedly exited"),
                Err(e) => tracing::error!("Sequencer server failed: {e:#}"),
            }
        }
    }
}

async fn run_sequencer(
    wal: BlockReplayWAL,
    state: StateHandle,
    mempool: Mempool,
) -> Result<()> {
    let last_block_in_wal = wal.latest_block().unwrap_or(0);
    tracing::info!(last_block_in_wal, "Last block in WAL: {last_block_in_wal}");

    command_source(&wal, last_block_in_wal)
        .map(Ok)                                         // ⇒ TryStream<Item = BlockCommand, Error = _>

        // ── Stage 1: execute_block ─────────────────────────────────────────
        .map_ok(|cmd| {
            let state = state.clone();
            let mempool = mempool.clone();
            async move {
                tracing::info!(block = cmd.block_number(), "▶ execute");
                execute_block(cmd, mempool, state.clone())
                    .await
                    .map(|(batch_out, replay)| {
                        state.handle_block_output(
                            batch_out.clone(),
                            replay.transactions.clone(),
                        );
                        (batch_out, replay)
                    }
                    ).context("execute_block")
            }
        })
        .try_buffered(1)

        // ── Stage 2: append_replay (canonise) ──────────────────────────────
        .try_for_each(|(batch_out, replay)| {
            let wal = wal.clone();
            let state = state.clone();
            async move {
                tracing::info!(block = batch_out.header.number, txs = replay.transactions.len(), "▶ append_replay");
                wal.append_replay(batch_out.clone(), replay.clone()).await;
                tracing::info!(block = batch_out.header.number, txs = replay.transactions.len(), "▶ advance_canonized_block");
                state.advance_canonized_block(batch_out.header.number);
                tracing::info!(block = batch_out.header.number, txs = replay.transactions.len(), "✔ done");

                Ok::<_, anyhow::Error>(())
            }
        })
        .await?;


    // pin_mut!(stream);
    //
    // stream.await;

    // let mut ts = std::time::Instant::now();

    // while let Some(command) = stream.next().await {
    //     let block_number = command.block_number();
    //
    //     tracing::info!("Block {} - {} - started loop iteration in {:?}, executing", block_number, command, ts.elapsed());
    //     ts = std::time::Instant::now();
    //     let (block_output, replay_record) = execute_block(
    //         command,
    //         mempool.clone(),
    //         state_handle.clone(),
    //     ).await?;
    //
    //     tracing::info!("Block {} ({}txs) - executed after {:?}, canonizing", block_number, replay_record.transactions.len(), ts.elapsed());
    //     ts = std::time::Instant::now();
    //
    //     let (block_output, replay_data) = block_replay_wal
    //         .append_replay(block_output, replay_record.clone())
    //         .await;
    //
    //     tracing::info!("Block {} ({}txs) - canonized after {:?}, saving", block_number, replay_record.transactions.len(), ts.elapsed());
    //     ts = std::time::Instant::now();
    //     state_handle.handle_block_output(
    //         block_output,
    //         replay_data.transactions,
    //     );
    //     tracing::info!("Block {} ({}txs) - saved after {:?}, next loop iteration", block_number, replay_record.transactions.len(), ts.elapsed());
    //     ts = std::time::Instant::now();
    // }
    Ok(())
}

fn command_source(block_replay_wal: &BlockReplayWAL, last_block_in_wal: u64) -> Chain<BoxStream<BlockCommand>, BoxStream<BlockCommand>> {
    // Stream of replay commands from WAL
    let replay_stream: BoxStream<BlockCommand> = Box::pin(block_replay_wal.replay_commands_from(1));

    // Stream of produce commands: pull-based, fetch context on demand
    let produce_stream: BoxStream<BlockCommand> =
        futures::stream::unfold(last_block_in_wal + 1, move |block_number| {
            async move {
                Some((BlockContextProvider.get_produce_command(block_number).await, block_number + 1))
            }
        }).boxed();

    // Combined source: run WAL replay first, then produce blocks from mempool
    let mut stream = replay_stream
        .chain(produce_stream);
    stream
}


