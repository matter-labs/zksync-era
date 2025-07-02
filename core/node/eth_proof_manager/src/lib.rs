use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

use crate::client::EthProofManagerClient;

mod client;
pub mod node;
mod sender;
mod types;
mod watcher;

#[derive(Debug)]
pub struct EthProofManager {
    watcher: watcher::EthProofWatcher,
    sender: sender::EthProofSender,
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
                client.clone_boxed(),
                connection_pool.clone(),
                blob_store.clone(),
                config.clone(),
            ),
            sender: sender::EthProofSender::new(
                client,
                connection_pool.clone(),
                blob_store.clone(),
                config.clone(),
                config.proof_generation_timeout,
                config.l2_chain_id,
            ),
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tokio::select! {
            _ = self.watcher.run(stop_receiver.clone()) => {
                tracing::info!("Watcher stopped");
            },
            _ = self.sender.run(stop_receiver) => {
                tracing::info!("Sender stopped");
            },
        }
        Ok(())
    }
}
