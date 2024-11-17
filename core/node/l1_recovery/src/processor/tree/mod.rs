pub mod tree_wrapper;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;
use zksync_basic_types::{web3::keccak256, AccountTreeId, L2BlockNumber, H256, U256};
use zksync_merkle_tree::TreeEntry;
use zksync_types::{
    block::unpack_block_info, snapshots::SnapshotStorageLog, StorageKey, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION, SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES,
};
use zksync_utils::{h256_to_u256, u256_to_h256};
use zksync_vm_interface::L2Block;

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
        initial_state_path: Option<PathBuf>,
    ) -> Result<Vec<TreeEntry>> {
        self.tree.insert_genesis_state(initial_state_path).await
    }

    pub fn get_inner_db(&self) -> Arc<Mutex<ReconstructionDatabase>> {
        self.inner_db.clone()
    }

    pub fn read_latest_miniblock_metadata(&self) -> L2Block {
        let l2_block_info_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
        );
        let packed_info = self.tree.read_storage_value(l2_block_info_key.hashed_key());
        let (number, timestamp) = unpack_block_info(h256_to_u256(packed_info));

        let position = h256_to_u256(SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION)
            + U256::from((number - 1) as u32 % SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        let l2_hash_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            u256_to_h256(position),
        );
        let prev_block_hash = self.tree.read_storage_value(l2_hash_key.hashed_key());

        let position = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
        );
        let txs_rolling_hash = self.tree.read_storage_value(position.hashed_key());

        let mut digest: [u8; 128] = [0u8; 128];
        U256::from(number).to_big_endian(&mut digest[0..32]);
        U256::from(timestamp).to_big_endian(&mut digest[32..64]);
        digest[64..96].copy_from_slice(prev_block_hash.as_bytes());
        digest[96..128].copy_from_slice(txs_rolling_hash.as_bytes());

        L2Block {
            number: number as u32,
            timestamp,
            hash: H256(keccak256(&digest)),
        }
    }

    pub fn get_root_hash(&self) -> RootHash {
        self.tree.get_root_hash()
    }
    pub async fn process_one_block(&mut self, block: CommitBlock) -> Vec<TreeEntry> {
        // Check if we've already processed this block.
        let latest_l1_batch = self
            .inner_db
            .lock()
            .await
            .get_latest_l1_batch_number()
            .expect("value should default to 0");
        if latest_l1_batch >= block.l1_batch_number {
            tracing::debug!(
                "Batch {} has already been processed, skipping.",
                block.l1_batch_number
            );
            return vec![];
        }

        let entries = self.tree.insert_block(&block).await.unwrap();

        // Update snapshot values.
        self.inner_db
            .lock()
            .await
            .set_latest_l1_batch_number(block.l1_batch_number)
            .expect("db failed");

        return entries;
    }
}
