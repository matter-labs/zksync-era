use std::sync::Arc;

use anyhow::Context as _;

use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_config::{ContractsConfig, GenesisConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_health_check::AppHealthCheck;
use zksync_node_genesis::GenesisParams;
use zksync_types::L1BatchNumber;
use zksync_web3_decl::client::{DynClient, L1};

use crate::{node_role::NodeRole, SnapshotRecoveryConfig};

#[derive(Debug)]
pub struct MainNodeRole {
    pub genesis: GenesisConfig,
    pub contracts: ContractsConfig,
    pub l1_client: Box<DynClient<L1>>,
}

#[async_trait::async_trait]
impl NodeRole for MainNodeRole {
    /// Will perform genesis initialization if it's required.
    /// If genesis is already performed, this method will do nothing.
    async fn genesis(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection().await.context("connection()")?;

        if !storage.blocks_dal().is_genesis_needed().await? {
            return Ok(());
        }

        let params = GenesisParams::load_genesis_params(self.genesis.clone())?;
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

    async fn is_genesis_performed(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<bool> {
        let mut storage = pool.connection().await.context("connection()")?;
        let needed = zksync_node_genesis::is_genesis_needed(&mut storage).await?;
        Ok(!needed)
    }

    async fn snapshot_recovery(
        &self,
        _stop_receiver: watch::Receiver<bool>,
        _pool: &ConnectionPool<Core>,
        _recovery_config: SnapshotRecoveryConfig,
        _app_health: Arc<AppHealthCheck>,
    ) -> anyhow::Result<()> {
        anyhow::bail!("Snapshot recovery is not supported for the main node")
    }

    async fn is_snapshot_recovery_completed(
        &self,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        anyhow::bail!("Snapshot recovery is not supported for the main node")
    }

    async fn should_rollback_to(
        &self,
        _stop_receiver: watch::Receiver<bool>,
        _pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<Option<L1BatchNumber>> {
        // Automatic rollback is not supported for the main node.
        Ok(None)
    }

    async fn perform_rollback(
        &self,
        _reverter: BlockReverter,
        _to_batch: L1BatchNumber,
    ) -> anyhow::Result<()> {
        anyhow::bail!("Automatic rollback is not supported for the main node")
    }
}
