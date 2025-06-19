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
use anyhow::{Result, Context};
use futures_util::TryFutureExt;
use itertools::Itertools;
use tokio::sync::broadcast::channel;
use tokio::sync::watch;
use zksync_os_sequencer::{api::run_jsonrpsee_server, mempool::{forced_deposit_transaction, Mempool}, run_sequencer_actor, storage::{
    block_replay_storage::{BlockReplayColumnFamily, BlockReplayStorage},
    persistent_storage_map::{PersistentStorageMap, StorageMapCF},
    rocksdb_preimages::{PreimagesCF, RocksDbPreimages},
    StateHandle,
}, BLOCK_REPLAY_WAL_PATH, PREIMAGES_STORAGE_PATH, STATE_STORAGE_PATH, TREE_STORAGE_PATH};
use zksync_os_sequencer::execution::block_executor::execute_block;
use zksync_os_sequencer::model::{BlockCommand, ReplayRecord};
use zksync_os_sequencer::tree_manager::TreeManager;
use zksync_storage::{RocksDB, RocksDBOptions, StalledWritesRetries};
use zksync_vlog::prometheus::PrometheusExporterConfig;
use zksync_zk_os_merkle_tree::RocksDBWrapper;

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

    let mut state_db = RocksDB::<StorageMapCF>::new(Path::new(STATE_STORAGE_PATH))
        .expect("Failed to open State DB");
    // state_db = state_db.with_sync_writes();
    let persistent_storage_map = PersistentStorageMap::new(state_db);


    let mut preimages_db = RocksDB::<PreimagesCF>::new(Path::new(PREIMAGES_STORAGE_PATH))
        .expect("Failed to open Preimages DB");
    // preimages_db = preimages_db.with_sync_writes();
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

    // Sequencer will not run the tree - batcher will (other component, other machine)
    // running it for now just to test the performance
    let db = RocksDB::with_options(
        Path::new(TREE_STORAGE_PATH),
        RocksDBOptions {
            block_cache_capacity: Some(128 << 20),
            include_indices_and_filters_in_block_cache: false,
            large_memtable_capacity: Some(256 << 20),
            stalled_writes_retries: StalledWritesRetries::new(Duration::from_secs(10)),
            max_open_files: None,
        },
    ).unwrap();
    let tree_wrapper = RocksDBWrapper::from(db);
    let mut tree_manager = TreeManager::new(
        tree_wrapper,
        state_handle.clone(),
        // this is a lie - we don't know the actual last block that was processed before restaty
        // but we only use tree for performance measure so its ok
        state_handle.last_canonized_block_number()
    );

    tokio::select! {
        // todo: only start after sequence caught up?
        // ── JSON-RPC task ────────────────────────────────────────────────
        res = run_jsonrpsee_server(state_handle.clone(), mempool.clone(), block_replay_storage.clone()) => {
            match res {
                Ok(_)  => tracing::warn!("JSON-RPC server unexpectedly exited"),
                Err(e) => tracing::error!("JSON-RPC server failed: {e:#}"),
            }
        }

        // ── TREE task ────────────────────────────────────────────────
        // res = tree_manager.run_loop() => {
        //     match res {
        //         Ok(_)  => tracing::warn!("TREE server unexpectedly exited"),
        //         Err(e) => tracing::error!("TREE server failed: {e:#}"),
        //     }
        // }

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

        _ = state_handle.collect_state_metrics(Duration::from_secs(2)) => {
            tracing::warn!("collect_state_metrics unexpectedly exited")
        }
        _ = state_handle.compact_periodically(Duration::from_millis(100)) => {
            tracing::warn!("compact_periodically unexpectedly exited")
        }

    }
}

