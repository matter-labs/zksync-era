use tokio::runtime::Handle;
use zksync_dal::{Core, CoreDal};
use zksync_db_connection::connection_pool::ConnectionPool;
use zksync_merkle_tree::RetainedVersionSource;

#[derive(Debug)]
pub struct KeepPruningSyncedWithDbPruning {
    pub pool: ConnectionPool<Core>,
}

impl RetainedVersionSource for KeepPruningSyncedWithDbPruning {
    fn target_retained_version(&self, _last_prunable_version: u64) -> anyhow::Result<u64> {
        let rt_handle = Handle::current();
        //this code looks awkward as tree crate is not using async
        let mut storage = rt_handle.block_on(self.pool.connection_tagged("tree_pruner"))?;
        // TODO update this code once db pruning is merged
        let _ = rt_handle.block_on(storage.blocks_dal().get_earliest_l1_batch_number())?;
        Ok(0)
    }
}
