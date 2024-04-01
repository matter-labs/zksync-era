use tokio::runtime::Handle;
use zksync_dal::{Core, CoreDal};
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_merkle_tree::RetainedVersionSource;
use zksync_types::L1BatchNumber;

#[derive(Debug)]
pub struct KeepPruningSyncedWithDbPruning {
    pub pool: ConnectionPool<Core>,
    pub rt_handle: Handle,
}

impl RetainedVersionSource for KeepPruningSyncedWithDbPruning {
    fn target_retained_version(&self, _last_prunable_version: u64) -> anyhow::Result<u64> {
        // We have to do this as as the whole tree does not work inside tokio runtime
        let target_l1_batch_number = self.rt_handle.block_on(self.get_l1_batch_to_prune())?.0;
        Ok(target_l1_batch_number as u64)
    }
}

impl KeepPruningSyncedWithDbPruning {
    async fn get_l1_batch_to_prune(&self) -> anyhow::Result<L1BatchNumber> {
        let mut storage = self.pool.connection_tagged("tree_pruner").await?;
        // TODO update this code once db pruning is merged
        let _ = storage
            .blocks_dal()
            .get_earliest_l1_batch_number()
            .await?
            .unwrap_or(L1BatchNumber(0));
        Ok(L1BatchNumber(0))
    }
}
