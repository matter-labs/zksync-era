use std::path::PathBuf;

use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{FromContext, IntoContext, WiringError, WiringLayer};

use super::resources::BlockReverterResource;
use crate::{BlockReverter, NodeRole};

/// Layer for the block reverter resource.
/// For documentation on the methods see the corresponding methods in [`BlockReverter`].
#[derive(Debug)]
pub struct BlockReverterLayer {
    node_role: NodeRole,
    allow_rolling_back_executed_batches: bool,
    should_roll_back_postgres: bool,
    state_keeper_cache_path: Option<PathBuf>,
    merkle_tree_path: Option<PathBuf>,
}

impl BlockReverterLayer {
    pub fn new(node_role: NodeRole) -> Self {
        Self {
            node_role,
            allow_rolling_back_executed_batches: false,
            should_roll_back_postgres: false,
            state_keeper_cache_path: None,
            merkle_tree_path: None,
        }
    }

    pub fn allow_rolling_back_executed_batches(&mut self) -> &mut Self {
        self.allow_rolling_back_executed_batches = true;
        self
    }

    pub fn enable_rolling_back_postgres(&mut self) -> &mut Self {
        self.should_roll_back_postgres = true;
        self
    }

    pub fn enable_rolling_back_merkle_tree(&mut self, path: PathBuf) -> &mut Self {
        self.merkle_tree_path = Some(path);
        self
    }

    pub fn enable_rolling_back_state_keeper_cache(&mut self, path: PathBuf) -> &mut Self {
        self.state_keeper_cache_path = Some(path);
        self
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub block_reverter: BlockReverterResource,
}

#[async_trait::async_trait]
impl WiringLayer for BlockReverterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "block_reverter_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let mut block_reverter = BlockReverter::new(self.node_role, pool);
        if self.allow_rolling_back_executed_batches {
            block_reverter.allow_rolling_back_executed_batches();
        }
        if self.should_roll_back_postgres {
            block_reverter.enable_rolling_back_postgres();
        }
        if let Some(path) = self.merkle_tree_path {
            block_reverter.enable_rolling_back_merkle_tree(path);
        }
        if let Some(path) = self.state_keeper_cache_path {
            block_reverter.add_rocksdb_storage_path_to_rollback(path);
        }

        Ok(Output {
            block_reverter: block_reverter.into(),
        })
    }
}
