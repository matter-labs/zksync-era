use std::path::PathBuf;

use zksync_basic_types::{web3::Bytes, L1BatchNumber, H256, U256};
use zksync_types::snapshots::{SnapshotFactoryDependency, SnapshotStorageLog};

use crate::{
    l1_fetcher::types::CommitBlock,
    processor::tree::TreeProcessor,
    storage::{snapshot::SnapshotDatabase, snapshot_columns, INDEX_TO_KEY_MAP},
};

pub struct StateCompressor {
    database: SnapshotDatabase,
    tree_processor: TreeProcessor,
}

impl StateCompressor {
    pub async fn new(db_path: PathBuf) -> Self {
        let database = SnapshotDatabase::new(db_path.clone()).unwrap();

        let tree_processor = TreeProcessor::new(db_path.join("_tree").clone())
            .await
            .unwrap();

        Self {
            database,
            tree_processor,
        }
    }

    pub async fn export_factory_deps(&self) -> Vec<SnapshotFactoryDependency> {
        let factory_deps = self
            .database
            .cf_handle(snapshot_columns::FACTORY_DEPS)
            .unwrap();
        let mut iterator = self
            .database
            .iterator_cf(factory_deps, rocksdb::IteratorMode::Start);

        let mut result = vec![];
        while let Some(Ok((_, bs))) = iterator.next() {
            let factory_dep: SnapshotFactoryDependency = bincode::deserialize(&bs).unwrap();
            result.push(factory_dep);
        }
        result
    }

    pub async fn export_storage_logs(&self) -> Vec<SnapshotStorageLog> {
        let index_to_key_map = self.database.cf_handle(INDEX_TO_KEY_MAP).unwrap();
        let mut iterator = self
            .database
            .iterator_cf(index_to_key_map, rocksdb::IteratorMode::Start);

        let mut result = vec![];
        while let Some(Ok((_, key))) = iterator.next() {
            let key = U256::from_big_endian(&key);
            if let Ok(Some(entry)) = self.database.get_storage_log(&key) {
                result.push(entry);
            };
        }
        result
    }

    pub fn get_root_hash(&self) -> H256 {
        self.tree_processor.get_root_hash()
    }

    pub async fn process_genesis_state(&mut self, path_buf: Option<PathBuf>) {
        let initial_entries = self
            .tree_processor
            .process_genesis_state(path_buf)
            .await
            .unwrap();

        for entry in initial_entries {
            let mut buffer = [0; 32];
            entry.key.to_big_endian(&mut buffer);
            self.database
                .insert_storage_log(&mut SnapshotStorageLog {
                    key: H256::from(buffer),
                    value: entry.value,
                    l1_batch_number_of_initial_write: L1BatchNumber(0),
                    enumeration_index: entry.leaf_index,
                })
                .unwrap();
        }
    }

    pub async fn process_blocks(&mut self, blocks: Vec<CommitBlock>) {
        for block in blocks {
            self.process_one_block(block).await;
        }
    }

    pub async fn process_one_block(&mut self, block: CommitBlock) {
        let entries = self.tree_processor.process_one_block(block.clone()).await;
        // batch was already processed
        if entries.len() == 0 {
            return;
        }

        // Initial calldata.
        for (key, value) in &block.initial_storage_changes {
            let value = self
                .database
                .process_value(*key, *value)
                .expect("failed to get key from database");
            let mut hashed_key = [0_u8; 32];
            key.to_big_endian(&mut hashed_key);
            self.database
                .insert_storage_log(&mut SnapshotStorageLog {
                    key: hashed_key.into(),
                    value,
                    l1_batch_number_of_initial_write: L1BatchNumber(block.l1_batch_number as u32),
                    enumeration_index: 0,
                })
                .expect("failed to insert storage_log_entry");
        }

        // Repeated calldata.
        for (index, value) in &block.repeated_storage_changes {
            let index = usize::try_from(*index).expect("truncation failed");
            let key = self
                .database
                .get_key_from_index(index as u64)
                .expect("missing key");
            let value = self
                .database
                .process_value(U256::from_big_endian(&key[0..32]), *value)
                .expect("failed to get key from database");

            if self
                .database
                .update_storage_log_value(index as u64, value)
                .is_err()
            {
                let max_idx = self
                    .database
                    .get_last_repeated_key_index()
                    .expect("failed to get latest repeated key index");
                tracing::error!(
                    "failed to find key with index {}, last repeated key index: {}",
                    index,
                    max_idx
                );
            };
        }

        // Factory dependencies.
        //tracing::info!("Inserting factory {} deps", &block.factory_deps.len());
        for dep in block.factory_deps {
            self.database
                .insert_factory_dep(&SnapshotFactoryDependency {
                    bytecode: Bytes(dep),
                })
                .expect("failed to save factory dep");
        }
    }
}
