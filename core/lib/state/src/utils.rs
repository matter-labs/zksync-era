use std::path::PathBuf;

use anyhow::Context;
use zksync_storage::RocksDB;

use crate::StateKeeperColumnFamily;

/// Opens a RocksDB database with the given path and family descriptors used by state keeper.
///
/// # Errors
///
/// Propagates RocksDB I/O errors.
pub async fn open_state_keeper_rocksdb(
    path: PathBuf,
) -> anyhow::Result<RocksDB<StateKeeperColumnFamily>> {
    tokio::task::spawn_blocking(move || {
        RocksDB::new(&path).context("failed initializing state keeper RocksDB")
    })
    .await
    .context("panicked initializing state keeper RocksDB")?
}
