#![allow(warnings)]

pub mod api;
mod block_context_provider;
pub mod execution;
pub mod mempool;
pub mod model;
pub mod storage;
pub mod tree_manager;
mod tx_conversions;
mod util;

use std::{path::Path, pin::Pin, ptr::null, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use futures::{
    future::FutureExt,
    pin_mut,
    stream::{self, BoxStream, Chain, Fuse, Stream, StreamExt, TryStream, TryStreamExt},
};
use futures_util::TryFutureExt;
use itertools::Itertools;
use tokio::sync::{broadcast::channel, watch};
use zk_ee::utils::Bytes32;
use zk_os_forward_system::run::{BatchContext, BatchOutput};
use zksync_storage::RocksDB;
use zksync_types::{address_to_h256, Address, H256};
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::{
    api::run_jsonrpsee_server,
    block_context_provider::BlockContextProvider,
    execution::{block_executor::execute_block, metrics::EXECUTION_METRICS},
    mempool::{forced_deposit_transaction, Mempool},
    model::{BlockCommand, ReplayRecord, TransactionSource},
    storage::{
        block_replay_storage::{BlockReplayColumnFamily, BlockReplayStorage},
        persistent_storage_map::{PersistentStorageMap, StorageMapCF},
        rocksdb_preimages::{PreimagesCF, RocksDbPreimages},
        storage_map::StorageMap,
        StateHandle,
    },
};
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

pub const BLOCK_REPLAY_WAL_PATH: &str = "../chains/era/db/main/block_replay_wal";
pub const STATE_STORAGE_PATH: &str = "../chains/era/db/main/state";
pub const PREIMAGES_STORAGE_PATH: &str = "../chains/era/db/main/preimages";
pub const TREE_STORAGE_PATH: &str = "../chains/era/db/main/tree";
pub const CHAIN_ID: u64 = 270;

// Maximum number of per-block information stored in memory - and thus returned from API.
// Older blocks are discarded (or, in case of state diffs, compacted)
pub const BLOCKS_TO_RETAIN: usize = 512;

pub const JSON_RPC_ADDR: &str = "127.0.0.1:3050";

pub const BLOCK_TIME_MS: u64 = 150;

pub const MAX_TX_SIZE: usize = 100000;

pub const MAX_NONCE_AHEAD: u32 = 1000;

pub const DEFAULT_ETH_CALL_GAS: u32 = 10000000;

pub async fn run_sequencer_actor(
    block_to_start: u64,
    wal: BlockReplayStorage,
    state: StateHandle,
    mempool: Mempool,
) -> Result<()> {
    let wal_clone = wal.clone();
    let state_clone = state.clone();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<(BatchOutput, ReplayRecord)>(1);

    /* ------------------------------------------------------------------ */
    /*  Stage 1: execute VM and send results                              */
    /* ------------------------------------------------------------------ */
    let exec_loop = {
        let wal      = wal.clone();
        let mempool  = mempool.clone();
        let state    = state.clone();

        async move {
            let mut stream = command_source(&wal, block_to_start);

            while let Some(cmd) = stream.next().await {
                let bn = cmd.block_number();
                tracing::info!(block = bn, "▶ execute");

                let (batch_out, replay) =
                    execute_block(cmd, Box::pin(mempool.clone()), state.clone())
                        .await
                        .context("execute_block")?;

                state.handle_block_output(batch_out.clone(), replay.transactions.clone());
                tx.send((batch_out, replay))
                    .await
                    .map_err(|_| anyhow::anyhow!("canonise loop stopped"))?;

                EXECUTION_METRICS.sealed_block[&"execute"].set(bn);
            }
            Ok::<(), anyhow::Error>(())
        }
    };



    /* ------------------------------------------------------------------ */
    /*  Stage 2: canonise                                                 */
    /* ------------------------------------------------------------------ */
    let canonise_loop = async {
        while let Some((batch_out, replay)) = rx.recv().await {
            let bn = batch_out.header.number;
            tracing::info!(block = bn, "▶ append_replay");
            wal.append_replay(batch_out.clone(), replay).await;

            tracing::info!(block = bn, "▶ advance_canonized_block");
            state.advance_canonized_block(bn);
            EXECUTION_METRICS.sealed_block[&"canonize"].set(bn);
            tracing::info!(block = bn, "✔ done");
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        res = exec_loop      => res?,
        res = canonise_loop  => res?,
    }
    Ok(())
}


pub async fn run_sequencer_stream(
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
                execute_block(cmd, Box::pin(mempool.clone()), state.clone())
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

fn command_source(block_replay_wal: &BlockReplayStorage, block_to_start: u64) -> Chain<BoxStream<BlockCommand>, BoxStream<BlockCommand>> {

    let last_block_in_wal = block_replay_wal.latest_block().unwrap_or(0);
    tracing::info!(last_block_in_wal, "Last block in WAL: {last_block_in_wal}");

    // Stream of replay commands from WAL
    let replay_stream: BoxStream<BlockCommand> = Box::pin(block_replay_wal.replay_commands_from(block_to_start));

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
