use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::{ContractsConfig, GenesisConfig, ObjectStoreConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_node_genesis::GenesisParams;
use zksync_types::L1BatchNumber;
use zksync_web3_decl::client::{DynClient, L1};

#[derive(Debug)]
pub struct MainNodeRole {
    genesis: GenesisConfig,
    contracts: ContractsConfig,
    l1_client: Box<DynClient<L1>>,
}

impl MainNodeRole {
    pub(crate) async fn genesis(self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection().await.context("connection()")?;
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
}
