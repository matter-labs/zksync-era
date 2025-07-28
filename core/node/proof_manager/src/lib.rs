use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::proof_manager::ProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::L2ChainId;

use crate::client::BoxedProofManagerClient;

mod client;
pub mod node;
mod sender;
#[cfg(test)]
mod tests;
mod types;
mod watcher;

pub struct ProofManager {
    watcher: watcher::ProofWatcher,
    sender: sender::ProofSender,
}

impl ProofManager {
    pub fn new(
        client: Box<dyn BoxedProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        public_blob_store: Arc<dyn ObjectStore>,
        config: ProofManagerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            watcher: watcher::ProofWatcher::new(
                client.clone_boxed(),
                connection_pool.clone(),
                blob_store.clone(),
                config.clone(),
            ),
            sender: sender::ProofSender::new(
                client,
                connection_pool.clone(),
                blob_store.clone(),
                public_blob_store.clone(),
                config,
                l2_chain_id,
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
