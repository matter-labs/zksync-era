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
use std::ptr::null;
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
use crate::mempool::{forced_deposit_transaction, Mempool};
use crate::model::{BlockCommand, ReplayRecord, TransactionSource};
use crate::storage::block_replay_storage::{BlockReplayStorage};
use crate::storage::in_memory_state::{InMemoryStorage};
use crate::storage::StateHandle;
use anyhow::{Result, Context};
use itertools::Itertools;
use tokio::sync::broadcast::channel;
use tokio::sync::watch;
use zksync_vlog::prometheus::PrometheusExporterConfig;
use crate::execution::metrics::{EXECUTION_METRICS};

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

const MAX_NONCE_AHEAD : u32 = 1000;

const DEFAULT_ETH_CALL_GAS: u32 = 10000000;

#[tokio::main]
pub async fn main() {
    let prometheus: PrometheusExporterConfig =
        PrometheusExporterConfig::pull(3312);

    let (stop_sender, stop_receiver) = watch::channel(false);

    tokio::task::spawn(prometheus.run(stop_receiver));
    // let prometheus_task = self.config.run(stop_receiver.0);

    tracing_subscriber::fmt()
        // .pretty()
        .init();

    tracing::warn!("TODO: Current Implementation doesn't persist Block Recipies or State - so we replay all blocks starting from genesis");

    let block_replay_storage = BlockReplayStorage::new(Path::new(BLOCK_REPLAY_WAL_PATH))
        .expect("Failed to open BlockReplayWAL");

    let state_handle = StateHandle::empty();
    let mempool = Mempool::new(forced_deposit_transaction());

    tokio::select! {
        // ── JSON-RPC task ────────────────────────────────────────────────
        res = run_jsonrpsee_server(state_handle.clone(), mempool.clone(), block_replay_storage.clone()) => {
            match res {
                Ok(_)  => tracing::warn!("JSON-RPC server unexpectedly exited"),
                Err(e) => tracing::error!("JSON-RPC server failed: {e:#}"),
            }
        }

        // ── Sequencer task ───────────────────────────────────────────────
        res = run_sequencer_actor(block_replay_storage, state_handle, mempool) => {
            match res {
                Ok(_)  => tracing::warn!("Sequencer server unexpectedly exited"),
                Err(e) => tracing::error!("Sequencer server failed: {e:#}"),
            }
        }
    }
}

async fn run_sequencer_actor(
    wal: BlockReplayStorage,
    state: StateHandle,
    mempool: Mempool,
) -> Result<()> {
    let last_block_in_wal = wal.latest_block().unwrap_or(0);
    tracing::info!(last_block_in_wal, "Last block in WAL: {last_block_in_wal}");
    let wal_clone = wal.clone();
    let state_clone = state.clone();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<(BatchOutput, ReplayRecord)>(1);

    /* ── Stage 1: execute  (runs concurrently with canonising) ─────────── */
    tokio::spawn(async move {
        let mut stream = command_source(&wal_clone, last_block_in_wal);

        while let Some(cmd) = stream.next().await {
            let block_number = cmd.block_number();

            tracing::info!(block = block_number, cmd = cmd.to_string(), "▶ execute");
            let (batch_out, replay) =
                execute_block(cmd, mempool.clone(), state.clone())
                    .await
                    .context("execute_block")?;
            tracing::info!(block = block_number, txs_count = replay.transactions.len(), "▶ handle_block_output");

            state.handle_block_output(
                batch_out.clone(),
                replay.transactions.clone(),
            );
            tx.send((batch_out, replay)).await.ok();   // back-pressure here

            EXECUTION_METRICS.sealed_block[&"execute"].set(block_number);
        }
        Result::<()>::Ok(())
    });

    /* ── Stage 2: canonise  (runs concurrently with next VM) ─────────── */
    while let Some((batch_out, replay)) = rx.recv().await {
        let block_number = batch_out.header.number;

        tracing::info!(block = block_number, txs_count = replay.transactions.len(), "▶ append_replay");
        wal.clone().append_replay(batch_out.clone(), replay).await;
        tracing::info!(block = block_number, "▶ advance_canonized_block");
        state_clone.clone().advance_canonized_block(block_number);
        EXECUTION_METRICS.sealed_block[&"canonize"].set(batch_out.header.number);

        tracing::info!(block = block_number, "✔ done");
    }

    Ok(())
}


async fn run_sequencer_stream(
    wal: BlockReplayStorage,
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
        .map_ok(|(batch_out, replay)| {
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
        .try_buffered(1)
        .try_for_each(|()| async { Ok::<_, anyhow::Error>(()) })
        .await?;
    Ok(())
}

fn command_source(block_replay_wal: &BlockReplayStorage, last_block_in_wal: u64) -> Chain<BoxStream<BlockCommand>, BoxStream<BlockCommand>> {
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

