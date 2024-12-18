pub mod tree_wrapper;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;
use zksync_basic_types::H256;
use zksync_merkle_tree::TreeEntry;
use zksync_types::snapshots::SnapshotStorageLog;

use self::tree_wrapper::TreeWrapper;
use crate::{
    l1_fetcher::{constants::storage::INNER_DB_NAME, types::CommitBlock},
    storage::reconstruction::ReconstructionDatabase,
};

pub type RootHash = H256;

#[derive(Debug)]
pub struct TreeProcessor {
    /// The internal merkle tree.
    tree: TreeWrapper,
    /// The stored state snapshot.
    inner_db: Arc<Mutex<ReconstructionDatabase>>,
}

impl TreeProcessor {
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        let inner_db_path = db_path.join(INNER_DB_NAME);

        let new_state = ReconstructionDatabase::new(inner_db_path.clone())?;
        let inner_db = Arc::new(Mutex::new(new_state));
        let tree = TreeWrapper::new(&db_path, inner_db.clone()).await?;

        Ok(Self { tree, inner_db })
    }

    pub async fn process_snapshot_storage_logs(
        &mut self,
        storage_logs: Vec<SnapshotStorageLog>,
    ) -> Result<()> {
        self.tree.insert_snapshot_storage_logs(storage_logs).await
    }
    pub async fn process_genesis_state(
        &mut self,
        initial_state_path: PathBuf,
    ) -> Result<Vec<TreeEntry>> {
        self.tree
            .insert_genesis_state(Some(initial_state_path))
            .await
    }

    pub fn get_inner_db(&self) -> Arc<Mutex<ReconstructionDatabase>> {
        self.inner_db.clone()
    }

    pub fn get_root_hash(&self) -> RootHash {
        self.tree.get_root_hash()
    }
    pub async fn process_one_block(&mut self, block: CommitBlock) -> Vec<TreeEntry> {
        self.tree.insert_block(&block).await.unwrap()
    }
}
