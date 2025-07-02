use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

use crate::client::EthProofManagerClient;

#[derive(Debug)]
pub struct EthProofSender {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    config: EthProofManagerConfig,
}

impl EthProofSender {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        config: EthProofManagerConfig,
    ) -> Self {
        Self {
            client,
            connection_pool,
            blob_store,
            config,
        }
    }

    pub async fn run(&self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting eth proof sender");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                break;
            }

            // todo: implement

            tracing::info!(
                "Sleeping for {} seconds",
                self.config.request_sending_interval.as_secs()
            );
            tokio::time::sleep(self.config.request_sending_interval).await;
        }

        Ok(())
    }
}
