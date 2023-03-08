use tokio::sync::watch;
use tokio::task::JoinHandle;

use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_storage::RocksDB;

struct ProcessHandler(Option<(watch::Sender<bool>, JoinHandle<()>)>);

impl Drop for ProcessHandler {
    fn drop(&mut self) {
        let stop_sender_and_thread_handle = self.0.take();

        if let Some((stop_sender, _thread_handle)) = stop_sender_and_thread_handle {
            // We don't actually need to await `_thread_handle`
            // since sending stop signal must finish this task almost immediately
            let _ = stop_sender.send(true);
        }
    }
}

pub struct ServerHandler {
    process_handler: ProcessHandler,
}

impl ServerHandler {
    pub fn spawn_server(
        _db_path: String,
        _state_keeper_db: RocksDB,
        _config: ZkSyncConfig,
        _state_keeper_pool: ConnectionPool,
        _metadata_calculator_pool: ConnectionPool,
        _mempool_pool: ConnectionPool,
    ) -> Self {
        todo!("Spawning server is not implemented yet")
    }

    pub fn empty() -> Self {
        Self {
            process_handler: ProcessHandler(None),
        }
    }
}
