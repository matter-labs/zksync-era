use std::fs::File;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::{configs::contracts::SettlementLayerSpecificContracts, GenesisConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_node_genesis::GenesisParams;
use zksync_object_store::bincode;
use zksync_types::OrStopped;
use zksync_web3_decl::client::{DynClient, L1};

use crate::traits::InitializeStorage;

#[derive(Debug)]
pub struct MainNodeGenesis {
    pub genesis: GenesisConfig,
    pub contracts: SettlementLayerSpecificContracts,
    pub l1_client: Box<DynClient<L1>>,
    pub pool: ConnectionPool<Core>,
    pub event_expiration_blocks: u64,
}

impl MainNodeGenesis {
    async fn initialize(&self) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("genesis").await?;

        if !storage.blocks_dal().is_genesis_needed().await? {
            return Ok(());
        }

        let params = GenesisParams::load_genesis_params(self.genesis.clone())?;
        zksync_node_genesis::validate_genesis_params(
            &params,
            &self.l1_client,
            self.contracts.chain_contracts_config.diamond_proxy_addr,
        )
        .await?;

        let custom_genesis_state_reader = match &self.genesis.custom_genesis_state_path {
            Some(path) => match File::open(path) {
                Ok(file) => Some(bincode::deserialize_from(file)?),
                Err(e) => return Err(e.into()), // Propagate other errors
            },
            None => None,
        };

        zksync_node_genesis::ensure_genesis_state(
            &mut storage,
            &params,
            custom_genesis_state_reader,
        )
        .await?;

        zksync_node_genesis::save_set_chain_id_tx(
            &mut storage,
            &self.l1_client,
            self.contracts.chain_contracts_config.diamond_proxy_addr,
            self.event_expiration_blocks,
        )
        .await
        .context("Failed to save SetChainId upgrade transaction")?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl InitializeStorage for MainNodeGenesis {
    /// Will perform genesis initialization if it's required.
    /// If genesis is already performed, this method will do nothing.
    async fn initialize_storage(
        &self,
        _stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        self.initialize().await.map_err(Into::into)
    }

    async fn is_initialized(&self) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("genesis").await?;
        let needed = zksync_node_genesis::is_genesis_needed(&mut storage).await?;
        Ok(!needed)
    }
}
