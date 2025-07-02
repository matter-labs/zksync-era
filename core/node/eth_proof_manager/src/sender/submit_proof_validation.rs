use std::sync::Arc;

use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

use crate::client::EthProofManagerClient;

pub struct SubmitProofValidationSubmitter {
    client: Box<dyn EthProofManagerClient>,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: EthProofManagerConfig,
}

impl SubmitProofValidationSubmitter {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        blob_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Core>,
        config: EthProofManagerConfig,
    ) -> Self {
        Self {
            client,
            blob_store,
            connection_pool,
            config,
        }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
