use std::{
    collections::HashMap,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use tokio::sync::Mutex;
use zksync_basic_types::{H256, U256};
use zksync_merkle_tree::{Database, Key, MerkleTree, RocksDBWrapper, TreeEntry};
use zksync_storage::{RocksDB, RocksDBOptions};
use zksync_types::snapshots::SnapshotStorageLog;

use super::RootHash;
use crate::{
    l1_fetcher::types::CommitBlock,
    processor::genesis::{get_genesis_state, reconstruct_genesis_state},
    storage::{reconstruction::ReconstructionDatabase, PackingType},
};

#[derive(Debug)]
pub struct TreeWrapper {
    index_to_key: HashMap<u64, U256>,
    key_to_value: HashMap<U256, H256>,
    tree: MerkleTree<RocksDBWrapper>,
    inner_db: Arc<Mutex<ReconstructionDatabase>>,
}

impl TreeWrapper {
    /// Attempts to create a new [`TreeWrapper`].
    pub async fn new(db_path: &Path, inner_db: Arc<Mutex<ReconstructionDatabase>>) -> Result<Self> {
        let db_opt = RocksDBOptions {
            max_open_files: NonZeroU32::new(2048),
            ..Default::default()
        };
        let db = RocksDBWrapper::from(RocksDB::with_options(db_path, db_opt)?);
        let tree = MerkleTree::new(db).unwrap();

        Ok(Self {
            index_to_key: HashMap::new(),
            key_to_value: HashMap::new(),
            tree,
            inner_db,
        })
    }

    pub async fn insert_snapshot_storage_logs(
        &mut self,
        storage_logs: Vec<SnapshotStorageLog>,
    ) -> Result<()> {
        let tree_entries = storage_logs
            .into_iter()
            .map(|log| {
                let entry = TreeEntry::new(
                    U256::from_little_endian(log.key.as_bytes()),
                    log.enumeration_index,
                    log.value,
                );
                entry
            })
            .collect();
        self.tree.extend(tree_entries).unwrap();
        Ok(())
    }

    pub async fn insert_genesis_state(
        &mut self,
        initial_state_path: Option<PathBuf>,
    ) -> Result<Vec<TreeEntry>> {
        let mut snapshot = self.inner_db.lock().await;
        load_genesis_state(&mut self.tree, &mut snapshot, initial_state_path)
    }
    /// Inserts a block into the tree and returns the root hash of the resulting state tree.
    pub async fn insert_block(&mut self, block: &CommitBlock) -> Result<Vec<TreeEntry>> {
        self.clear_known_base();
        let mut tree_entries = Vec::with_capacity(block.initial_storage_changes.len());
        // INITIAL CALLDATA.
        let mut index =
            block.index_repeated_storage_changes - (block.initial_storage_changes.len() as u64);
        for (key, value) in &block.initial_storage_changes {
            self.insert_known_key(index, *key);
            let value = self.process_value(*key, *value);

            tree_entries.push(TreeEntry::new(*key, index, value));
            self.inner_db
                .lock()
                .await
                .add_key(key)
                .expect("cannot add key");
            index += 1;
        }

        // REPEATED CALLDATA.
        for (index, value) in &block.repeated_storage_changes {
            let index = *index;
            // Index is 1-based so we subtract 1.
            let key = self
                .inner_db
                .lock()
                .await
                .get_key(index - 1)
                .expect("invalid key index");
            self.insert_known_key(index, key);
            let value = self.process_value(key, *value);

            tree_entries.push(TreeEntry::new(key, index, value));
        }

        let output = self.tree.extend(tree_entries.clone()).unwrap();
        let root_hash = output.root_hash;

        tracing::debug!(
            "Root hash of batch {} = {}",
            block.l1_batch_number,
            hex::encode(root_hash)
        );

        let root_hash_bytes = root_hash.as_bytes();
        if root_hash_bytes == block.new_state_root {
            tracing::info!("Successfully processed batch {}", block.l1_batch_number);

            Ok(tree_entries)
        } else {
            tracing::error!(
                "Root hash mismatch!\nLocal: {}\nPublished: {}",
                hex::encode(root_hash_bytes),
                hex::encode(&block.new_state_root)
            );
            let mut rollback_entries = Vec::with_capacity(self.index_to_key.len());
            for (index, key) in &self.index_to_key {
                let value = self.key_to_value.get(key).unwrap();
                rollback_entries.push(TreeEntry::new(*key, *index, *value));
            }

            self.tree.extend(rollback_entries).unwrap();
            panic!(
                "Root hash mismatch for block {}, expected {:?}, got {:?}",
                block.l1_batch_number, root_hash_bytes, block.new_state_root
            );
        }
    }

    fn process_value(&mut self, key: U256, value: PackingType) -> H256 {
        let version = self.tree.latest_version().unwrap_or_default();
        if let Ok(leaf) = self.tree.entries(version, &[key]) {
            let hash = leaf.last().unwrap().value;
            self.insert_known_value(key, hash);
            let existing_value = U256::from(hash.to_fixed_bytes());
            // NOTE: We're explicitly allowing over-/underflow as per the spec.
            let processed_value = match value {
                PackingType::NoCompression(v) | PackingType::Transform(v) => v,
                PackingType::Add(v) => existing_value.overflowing_add(v).0,
                PackingType::Sub(v) => existing_value.overflowing_sub(v).0,
            };
            let mut buffer = [0; 32];
            processed_value.to_big_endian(&mut buffer);
            H256::from(buffer)
        } else {
            panic!("no key found for version")
        }
    }

    fn clear_known_base(&mut self) {
        self.index_to_key.clear();
        self.key_to_value.clear();
    }

    fn insert_known_key(&mut self, index: u64, key: U256) {
        if let Some(old_key) = self.index_to_key.insert(index, key) {
            assert_eq!(old_key, key);
        }
    }

    fn insert_known_value(&mut self, key: U256, value: H256) {
        if let Some(old_value) = self.key_to_value.insert(key, value) {
            tracing::debug!(
                "Updated value at {:?} from {:?} to {:?}",
                key,
                old_value,
                value
            );
        }
    }

    pub fn get_root_hash(&self) -> RootHash {
        self.tree.latest_root_hash()
    }

    pub fn read_storage_value(&self, hashed_key: H256) -> H256 {
        let latest_version = self.tree.latest_version().unwrap();
        self.tree
            .entries(
                latest_version,
                &[Key::from(U256::from_little_endian(hashed_key.as_bytes()))],
            )
            .unwrap()
            .last()
            .unwrap()
            .value
    }
}

/// Attempts to reconstruct the genesis state from a CSV file.
fn load_genesis_state<D: Database>(
    tree: &mut MerkleTree<D>,
    snapshot: &mut ReconstructionDatabase,
    path: Option<PathBuf>,
) -> Result<Vec<TreeEntry>> {
    let tree_entries = if let Some(path) = path {
        reconstruct_genesis_state(path)
    } else {
        get_genesis_state()
    };
    for entry in &tree_entries {
        snapshot
            .add_key(&entry.key)
            .expect("cannot add genesis key");
    }
    tracing::info!(
        "Inserted {} genesis storage logs into tree",
        tree_entries.len()
    );
    let output = tree.extend(tree_entries.clone()).unwrap();
    tracing::trace!("Initial state root = {}", hex::encode(output.root_hash));

    Ok(tree_entries)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::{
        l1_fetcher::constants::{mainnet_initial_state_path, sepolia_initial_state_path},
        processor::tree::TreeProcessor,
    };

    #[tokio::test]
    async fn era_mainnet_genesis() {
        let temp_dir = TempDir::new().unwrap().into_path().join("db");
        let mut processor = TreeProcessor::new(temp_dir).await.unwrap();
        processor
            .process_genesis_state(mainnet_initial_state_path())
            .await
            .unwrap();

        //https://explorer.zksync.io/batch/0
        assert_eq!(
            "0xbc59c242d551e3939b9b2939b8b686efa77ba3833183045d548aa5f53357ba95",
            format!("{:?}", processor.get_root_hash())
        );
    }

    #[tokio::test]
    async fn era_boojnet_genesis() {
        let temp_dir = TempDir::new().unwrap().into_path().join("db");
        let mut processor = TreeProcessor::new(temp_dir).await.unwrap();
        processor
            .process_genesis_state(sepolia_initial_state_path())
            .await
            .unwrap();

        //https://sepolia.explorer.zksync.io/batch/0
        assert_eq!(
            "0xd2892ccfb454d0cfa21f9c769fcbbc8da7a8fc9faf8c597b9bdfffc1e5adb0f2",
            format!("{:?}", processor.get_root_hash())
        );
    }
}
