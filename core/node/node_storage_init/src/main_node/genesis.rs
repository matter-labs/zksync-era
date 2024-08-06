use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::{ContractsConfig, GenesisConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_node_genesis::GenesisParams;
use zksync_web3_decl::client::{DynClient, L1};

use crate::traits::InitializeStorage;

#[derive(Debug)]
pub struct MainNodeGenesis {
    pub genesis: GenesisConfig,
    pub contracts: ContractsConfig,
    pub l1_client: Box<DynClient<L1>>,
    pub pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl InitializeStorage for MainNodeGenesis {
    /// Will perform genesis initialization if it's required.
    /// If genesis is already performed, this method will do nothing.
    async fn initialize_storage(
        &self,
        _stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("genesis").await?;

        if !storage.blocks_dal().is_genesis_needed().await? {
            return Ok(());
        }

        let params = GenesisParams::load_genesis_params(self.genesis.clone())?;
        zksync_node_genesis::ensure_genesis_state(&mut storage, &params).await?;

        zksync_node_genesis::save_set_chain_id_tx(
            &mut storage,
            &self.l1_client,
            self.contracts.diamond_proxy_addr,
        )
        .await
        .context("Failed to save SetChainId upgrade transaction")?;

        Ok(())
    }

    async fn is_initialized(&self) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("genesis").await?;
        let needed = zksync_node_genesis::is_genesis_needed(&mut storage).await?;
        Ok(!needed)
    }
}
