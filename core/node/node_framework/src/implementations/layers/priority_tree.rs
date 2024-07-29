use anyhow::Context;
use zksync_dal::CoreDal;
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_types::l1::L1Tx;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        priority_merkle_tree::PriorityTreeResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

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

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub priority_tree: PriorityTreeResource,
}

#[async_trait::async_trait]
impl WiringLayer for PriorityTreeLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "priority_tree_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let connection_pool = input.master_pool.get().await.unwrap();
        let priority_op_hashes = connection_pool
            .connection()
            .await
            .context("priority_merkle_tree")?
            .transactions_dal()
            .get_l1_transactions_hashes(self.priority_tree_start_index)
            .await
            .context("get_l1_transactions_hashes")?;
        let priority_tree: SyncMerkleTree<L1Tx> =
            SyncMerkleTree::from_hashes(priority_op_hashes.into_iter(), None);
        Ok(Output {
            priority_tree: PriorityTreeResource(priority_tree),
        })
    }
}
