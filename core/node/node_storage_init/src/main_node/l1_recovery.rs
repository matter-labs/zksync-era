use tokio::sync::watch::Receiver;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_web3_decl::client::{DynClient, L1};

use crate::InitializeStorage;

#[derive(Debug)]
pub struct L1Recovery {
    pub l1_client: Box<DynClient<L1>>,
    pub pool: ConnectionPool<Core>,
}
#[async_trait::async_trait]
impl InitializeStorage for L1Recovery {
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

    async fn initialize_storage(&self, stop_receiver: Receiver<bool>) -> anyhow::Result<()> {
        todo!()
    }
}
