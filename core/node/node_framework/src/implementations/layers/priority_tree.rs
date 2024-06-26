use anyhow::Context;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        priority_merkle_tree::PriorityTreeResource,
    },
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};
use zksync_dal::CoreDal;
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_types::l1::L1Tx;

#[derive(Debug)]
pub struct PriorityTreeLayer {
    priority_tree_start_index: usize,
}

impl PriorityTreeLayer {
    pub fn new(priority_tree_start_index: usize) -> Self {
        Self {
            priority_tree_start_index,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PriorityTreeLayer {
    fn layer_name(&self) -> &'static str {
        "priority_tree_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let connection_pool = context.get_resource::<PoolResource<MasterPool>>().await?;
        let priority_op_hashes = connection_pool
            .get()
            .await?
            .connection()
            .await
            .context("priority_merkle_tree")?
            .transactions_dal()
            .get_l1_transactions_hashes(self.priority_tree_start_index)
            .await
            .context("get_l1_transactions_hashes")?;
        let priority_tree: SyncMerkleTree<L1Tx> =
            SyncMerkleTree::from_hashes(priority_op_hashes.into_iter(), None);
        context.insert_resource(PriorityTreeResource(priority_tree))?;
        Ok(())
    }
}
