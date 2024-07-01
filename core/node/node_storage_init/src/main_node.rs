use anyhow::Context as _;

use zksync_config::{ContractsConfig, GenesisConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_node_genesis::GenesisParams;
use zksync_web3_decl::client::{DynClient, L1};

#[derive(Debug)]
pub struct MainNodeRole {
    genesis: GenesisConfig,
    contracts: ContractsConfig,
    l1_client: Box<DynClient<L1>>,
}

impl MainNodeRole {
    /// Will perform genesis initialization if it's required.
    /// If genesis is already performed, this method will do nothing.
    pub(crate) async fn genesis(self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection().await.context("connection()")?;

        if !storage.blocks_dal().is_genesis_needed().await? {
            return Ok(());
        }

        let params = GenesisParams::load_genesis_params(self.genesis)?;
        zksync_node_genesis::ensure_genesis_state(&mut storage, &params).await?;

        if let Some(ecosystem_contracts) = &self.contracts.ecosystem_contracts {
            zksync_node_genesis::save_set_chain_id_tx(
                &mut storage,
                &self.l1_client,
                self.contracts.diamond_proxy_addr,
                ecosystem_contracts.state_transition_proxy_addr,
            )
            .await
            .context("Failed to save SetChainId upgrade transaction")?;
        }

        Ok(())
    }

    pub(crate) async fn is_genesis_performed(
        &self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        let mut storage = pool.connection().await.context("connection()")?;
        let needed = zksync_node_genesis::is_genesis_needed(&mut storage).await?;
        Ok(!needed)
    }
}
