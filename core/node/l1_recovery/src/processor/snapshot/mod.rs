use std::{fs, path::PathBuf, sync::Arc};

use serde_json::Value;
use tokio::sync::watch;
use zksync_basic_types::{
    bytecode::BytecodeHash,
    h256_to_u256, u256_to_h256,
    web3::{keccak256, Bytes},
    AccountTreeId, L1BatchNumber, H256, U256,
};
use zksync_object_store::ObjectStore;
use zksync_types::{
    block::unpack_block_info,
    snapshots::{
        SnapshotFactoryDependency, SnapshotStorageLog, SnapshotStorageLogsChunk,
        SnapshotStorageLogsStorageKey,
    },
    StorageKey, SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION,
    SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES,
};
use zksync_vm_interface::L2Block;

use crate::{
    l1_fetcher::types::CommitBlock,
    processor::{genesis::get_genesis_factory_deps, tree::TreeProcessor},
    storage::{snapshot::SnapshotDatabase, snapshot_columns, INDEX_TO_KEY_MAP},
};

pub struct StateCompressor {
    database: SnapshotDatabase,
    tree_processor: TreeProcessor,
    tree_disabled: bool,
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
            tree_disabled: false,
        }
    }

    pub fn disabled_tree(&mut self) {
        self.tree_disabled = true;
    }

    pub async fn export_factory_deps(&self) -> Vec<SnapshotFactoryDependency> {
        let factory_deps = self
            .database
            .cf_handle(snapshot_columns::FACTORY_DEPS)
            .unwrap();
        let mut iterator = self
            .database
            .iterator_cf(factory_deps, rocksdb::IteratorMode::Start);

        let mut result: Vec<SnapshotFactoryDependency> = get_genesis_factory_deps()
            .iter()
            .map(|dep| SnapshotFactoryDependency {
                hash: Some(BytecodeHash::for_bytecode(dep).value()),
                bytecode: dep.clone().into(),
            })
            .collect();

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

    pub async fn dump_storage_logs_chunked(
        &self,
        last_l1_batch_number: L1BatchNumber,
        object_store: &Arc<dyn ObjectStore>,
    ) -> u64 {
        let index_to_key_map = self.database.cf_handle(INDEX_TO_KEY_MAP).unwrap();
        let mut iterator = self
            .database
            .iterator_cf(index_to_key_map, rocksdb::IteratorMode::Start);

        let mut result = vec![];
        let chunk_size = 1_000_000;
        let mut chunk_id = 0;
        loop {
            let mut last_key = false;
            if let Some(Ok((_, key))) = iterator.next() {
                let key = U256::from_big_endian(&key);
                if let Ok(Some(entry)) = self.database.get_storage_log(&key) {
                    result.push(entry);
                };
            } else {
                last_key = true;
            }
            if result.len() == chunk_size || (last_key && !result.is_empty()) {
                let key = SnapshotStorageLogsStorageKey {
                    l1_batch_number: last_l1_batch_number,
                    chunk_id,
                };
                let storage_logs = SnapshotStorageLogsChunk {
                    storage_logs: result.clone(),
                };
                result = vec![];
                object_store.put(key, &storage_logs).await.unwrap();
                tracing::info!(
                    "Dumped {} storage logs for chunk {chunk_id}",
                    storage_logs.storage_logs.len()
                );
                chunk_id += 1;
            }
            if last_key {
                break;
            }
        }

        chunk_id
    }

    pub fn get_root_hash(&self) -> H256 {
        assert!(!self.tree_disabled);
        self.tree_processor.get_root_hash()
    }

    fn read_storage_value(&self, hashed_key: H256) -> H256 {
        let key = U256::from_big_endian(&hashed_key.0);
        let value = self.database.get_storage_log(&key).unwrap();
        value.map(|v| v.value).unwrap_or_default()
    }
    pub fn read_latest_miniblock_metadata(&self) -> L2Block {
        let l2_block_info_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
        );
        let packed_info = self.read_storage_value(l2_block_info_key.hashed_key());
        let (number, timestamp) = unpack_block_info(h256_to_u256(packed_info));

        let position = h256_to_u256(SYSTEM_CONTEXT_CURRENT_L2_BLOCK_HASHES_POSITION)
            + U256::from((number - 1) as u32 % SYSTEM_CONTEXT_STORED_L2_BLOCK_HASHES);
        let l2_hash_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            u256_to_h256(position),
        );
        let prev_block_hash = self.read_storage_value(l2_hash_key.hashed_key());

        let position = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
        );
        let txs_rolling_hash = self.read_storage_value(position.hashed_key());
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

    fn insert_genesis_factory_deps(&mut self, path_buf: PathBuf) {
        let input = fs::read_to_string(path_buf.clone())
            .unwrap_or_else(|_| panic!("Unable to read initial state from {path_buf:?}"));
        let data: Value = serde_json::from_str(&input).unwrap();
        let factory_deps = data.get("factory_deps").unwrap();

        let mut processed_deps = vec![];
        for factory_dep in factory_deps.as_array().unwrap() {
            let bytecode_str: &str = factory_dep
                .get("bytecode")
                .unwrap()
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap();
            let bytecode = hex::decode(bytecode_str).unwrap();

            processed_deps.push(bytecode)
        }

        for factory_dep in processed_deps {
            self.database
                .insert_factory_dep(&SnapshotFactoryDependency {
                    hash: Some(BytecodeHash::for_bytecode(&factory_dep).value()),
                    bytecode: Bytes(factory_dep),
                })
                .expect("failed to save factory dep");
        }
    }
    pub async fn process_genesis_state(&mut self, path_buf: PathBuf) {
        let initial_entries = self
            .tree_processor
            .process_genesis_state(path_buf.clone())
            .await
            .unwrap();

        for entry in initial_entries {
            let mut buffer = [0; 32];
            entry.key.to_little_endian(&mut buffer);
            self.database
                .insert_storage_log(&mut SnapshotStorageLog {
                    key: H256::from(buffer),
                    value: entry.value,
                    l1_batch_number_of_initial_write: L1BatchNumber(0),
                    enumeration_index: entry.leaf_index,
                })
                .unwrap();
        }
        self.insert_genesis_factory_deps(path_buf);
    }

    pub async fn process_blocks(
        &mut self,
        blocks: Vec<CommitBlock>,
        stop_receiver: &watch::Receiver<bool>,
    ) {
        for block in blocks {
            self.process_one_block(block, stop_receiver).await;
        }
    }

    pub async fn process_one_block(
        &mut self,
        block: CommitBlock,
        stop_receiver: &watch::Receiver<bool>,
    ) {
        if *stop_receiver.borrow() {
            panic!("Stop requested");
        }

        if !self.tree_disabled {
            self.tree_processor.process_one_block(block.clone()).await;
        }

        // Initial calldata.
        for (key, value) in &block.initial_storage_changes {
            let value = self
                .database
                .process_value(*key, *value)
                .expect("failed to get key from database");
            let mut hashed_key = [0_u8; 32];
            key.to_little_endian(&mut hashed_key);
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

            self.database
                .update_storage_log_value(index as u64, value)
                .unwrap();
        }

        // Factory dependencies.
        //tracing::info!("Inserting factory {} deps", &block.factory_deps.len());
        for dep in block.factory_deps {
            self.database
                .insert_factory_dep(&SnapshotFactoryDependency {
                    hash: Some(BytecodeHash::for_bytecode(&dep).value()),
                    bytecode: Bytes(dep),
                })
                .expect("failed to save factory dep");
        }

        if block.l1_batch_number % 100 == 0 {
            tracing::info!("Processed block {} for snapshot", block.l1_batch_number,);
        }
    }
}
