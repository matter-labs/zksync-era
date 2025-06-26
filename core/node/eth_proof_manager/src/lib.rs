use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

use crate::client::EthProofManagerClient;

mod client;
mod types;
mod watcher;

pub struct EthProofManager {
    watcher: watcher::EthProofWatcher,
}

impl EthProofManager {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            watcher: watcher::EthProofWatcher::new(
                client,
                connection_pool,
                blob_store,
                poll_interval,
            ),
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.watcher.run(stop_receiver).await
    }
}
