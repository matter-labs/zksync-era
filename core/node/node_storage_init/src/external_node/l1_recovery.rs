use tokio::sync::watch::Receiver;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_l1_recovery::recover_db;
use zksync_types::Address;
use zksync_web3_decl::client::{DynClient, L1};

use crate::InitializeStorage;

#[derive(Debug)]
pub struct ExternalNodeL1Recovery {
    pub l1_client: Box<DynClient<L1>>,
    pub pool: ConnectionPool<Core>,
    pub diamond_proxy_address: Address,
}
#[async_trait::async_trait]
impl InitializeStorage for ExternalNodeL1Recovery {
    async fn is_initialized(&self) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("genesis").await?;
        let needed = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .unwrap()
            .is_some();
        Ok(!needed)
    }

    async fn initialize_storage(&self, _stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        recover_db(
            self.pool.clone(),
            self.l1_client.clone().for_component("l1_recovery"),
            self.diamond_proxy_address,
        )
        .await;
        Ok(())
    }
}
