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
use crate::storage::block_replay_storage::{BlockReplayColumnFamily, BlockReplayStorage};
use crate::storage::storage_map::{StorageMap};
use crate::storage::StateHandle;
use anyhow::{Result, Context};
use futures_util::TryFutureExt;
use itertools::Itertools;
use tokio::sync::broadcast::channel;
use tokio::sync::watch;
use zksync_storage::RocksDB;
use zksync_vlog::prometheus::PrometheusExporterConfig;
use crate::execution::metrics::{EXECUTION_METRICS};
use crate::storage::persistent_storage_map::{PersistentStorageMap, StorageMapCF};
use crate::storage::rocksdb_preimages::{PreimagesCF, RocksDbPreimages};
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
const STATE_STORAGE_PATH: &str = "../chains/era/db/main/state";
const PREIMAGES_STORAGE_PATH: &str = "../chains/era/db/main/preimages";
const CHAIN_ID: u64 = 270;

// Maximum number of per-block information stored in memory - and thus returned from API.
// Older blocks are discarded (or, in case of state diffs, compacted)
const BLOCKS_TO_RETAIN: usize = 128;

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
    tokio::task::spawn(prometheus.run(stop_receiver).map_ok(|o| tracing::error!("unexp")).map_err(|e| {
        tracing::error!("Prometheus exporter failed: {e:#}");
    }));

    tracing_subscriber::fmt().init();
        // .pretty()

    let block_replay_storage_rocks_db = RocksDB::<BlockReplayColumnFamily>::new(Path::new(BLOCK_REPLAY_WAL_PATH))
        .expect("Failed to open BlockReplayWAL");

    let block_replay_storage = BlockReplayStorage::new(block_replay_storage_rocks_db);

    let state_db = RocksDB::<StorageMapCF>::new(Path::new(STATE_STORAGE_PATH))
        .expect("Failed to open State DB");
    let persistent_storage_map = PersistentStorageMap::new(state_db);


    let preimages_db = RocksDB::<PreimagesCF>::new(Path::new(PREIMAGES_STORAGE_PATH))
        .expect("Failed to open Preimages DB");
    let rocks_db_preimages = RocksDbPreimages::new(preimages_db);

    let state_db_block = persistent_storage_map.rocksdb_block_number();
    let preimages_db_block = rocks_db_preimages.rocksdb_block_number();
    assert!(
        state_db_block <= preimages_db_block,
        "State DB block number ({state_db_block}) is greater than Preimages DB block number ({preimages_db_block}). This is not allowed."
    );

    let state_handle = StateHandle::empty(
        state_db_block,
        persistent_storage_map,
        rocks_db_preimages,
    );

    let block_to_start = state_db_block + 1;
    tracing::info!(
        "State DB block number: {state_db_block}, Preimages DB block number: {preimages_db_block}, starting execution from {block_to_start}"
    );

    let mempool = Mempool::new(forced_deposit_transaction());

    tokio::select! {
        // todo: only start after sequence caught up?
        // ── JSON-RPC task ────────────────────────────────────────────────
        res = run_jsonrpsee_server(state_handle.clone(), mempool.clone(), block_replay_storage.clone()) => {
            match res {
                Ok(_)  => tracing::warn!("JSON-RPC server unexpectedly exited"),
                Err(e) => tracing::error!("JSON-RPC server failed: {e:#}"),
            }
        }

        // ── Sequencer task ───────────────────────────────────────────────
        res = run_sequencer_actor(
            block_to_start,
            block_replay_storage,
            state_handle.clone(),
            mempool
        ) => {
            match res {
                Ok(_)  => tracing::warn!("Sequencer server unexpectedly exited"),
                Err(e) => tracing::error!("Sequencer server failed: {e:#}"),
            }
        }

        res = state_handle.collect_state_metrics() => {
            tracing::warn!("collect_state_metrics unexpectedly exited")
        }
    }
}

async fn run_sequencer_actor(
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
                    execute_block(cmd, mempool.clone(), state.clone())
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

