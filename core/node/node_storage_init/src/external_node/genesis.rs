use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_types::{L2ChainId, OrStopped};
use zksync_web3_decl::client::{DynClient, L2};

use crate::InitializeStorage;

#[derive(Debug)]
pub struct ExternalNodeGenesis {
    pub l2_chain_id: L2ChainId,
    pub client: Box<DynClient<L2>>,
    pub pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl InitializeStorage for ExternalNodeGenesis {
    /// Will perform genesis initialization if it's required.
    /// If genesis is already performed, this method will do nothing.
    async fn initialize_storage(
        &self,
        _stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        let mut storage = self.pool.connection_tagged("en").await?;

        // Custom genesis state for external nodes is not supported. If the main node has a custom genesis,
        // its external nodes should be started from a Postgres dump or a snapshot instead.
        zksync_node_sync::genesis::perform_genesis_if_needed(
            &mut storage,
            self.l2_chain_id,
            &self.client.clone().for_component("genesis"),
            None,
        )
        .await
        .context("performing genesis failed")?;

        Ok(())
    }

    async fn is_initialized(&self) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("en").await?;
        let needed = zksync_node_sync::genesis::is_genesis_needed(&mut storage).await?;
        Ok(!needed)
    }
}
