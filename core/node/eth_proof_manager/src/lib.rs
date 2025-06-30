use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

use crate::client::EthProofManagerClient;

mod client;
pub mod node;
mod types;
mod watcher;

#[derive(Debug)]
pub struct EthProofManager {
    watcher: watcher::EthProofWatcher,
}

impl EthProofManager {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        config: EthProofManagerConfig,
    ) -> Self {
        Self {
            watcher: watcher::EthProofWatcher::new(
                client,
                connection_pool,
                blob_store,
                config,
            ),
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.watcher.run(stop_receiver).await
    }
}
