//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use std::collections::BTreeMap;
use std::time::Instant;

use tokio::sync::watch;

use zksync_config::{DBConfig, ZkSyncConfig};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_merkle_tree::{TreeMetadata, TreeMode, ZkSyncTree};
use zksync_object_store::gcs_utils::merkle_tree_paths_blob_url;
use zksync_object_store::object_store::{
    create_object_store_from_env, DynamicObjectStore, WITNESS_INPUT_BUCKET_PATH,
};
use zksync_storage::db::Database;
use zksync_storage::rocksdb::backup::{BackupEngine, BackupEngineOptions, RestoreOptions};
use zksync_storage::rocksdb::{Options, DB};
use zksync_storage::RocksDB;
use zksync_types::block::L1BatchHeader;
use zksync_types::commitment::{BlockCommitment, BlockMetadata, BlockWithMetadata};
use zksync_types::{
    block::WitnessBlockWithLogs, L1BatchNumber, StorageKey, StorageLog, StorageLogKind,
    WitnessStorageLog, H256,
};
use zksync_utils::time::seconds_since_epoch;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct MetadataCalculator {
    #[cfg_attr(test, allow(dead_code))]
    delay_interval: std::time::Duration,
    tree: ZkSyncTree,
    config: DBConfig,
    mode: MetadataCalculatorMode,
    object_store: DynamicObjectStore,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MetadataCalculatorMode {
    Full,
    Lightweight,
    Backup,
}

impl From<MetadataCalculatorMode> for TreeMode {
    fn from(mode: MetadataCalculatorMode) -> Self {
        match mode {
            MetadataCalculatorMode::Lightweight => TreeMode::Lightweight,
            _ => TreeMode::Full,
        }
    }
}

impl MetadataCalculator {
    pub fn new(config: &ZkSyncConfig, mode: MetadataCalculatorMode) -> Self {
        {
            let db = RocksDB::new(
                Database::MerkleTree,
                Self::rocksdb_path(&config.db, mode),
                false,
            );
            let tree = ZkSyncTree::new(db);
            if tree.is_empty() {
                Self::restore_from_backup(&config.db);
            }
        }
        let db = RocksDB::new(
            Database::MerkleTree,
            Self::rocksdb_path(&config.db, mode),
            true,
        );
        let tree = ZkSyncTree::new_with_mode(db, mode.into());
        Self {
            delay_interval: config.chain.operations_manager.delay_interval(),
            tree,
            config: config.db.clone(),
            mode,
            object_store: create_object_store_from_env(),
        }
    }

    pub async fn run(mut self, pool: ConnectionPool, stop_receiver: watch::Receiver<bool>) {
        let mut storage = pool.access_storage().await;

        // ensure genesis creation
        if self.tree.is_empty() {
            let storage_logs = get_logs_for_l1_batch(&mut storage, L1BatchNumber(0));
            self.tree.process_block(storage_logs.unwrap().storage_logs);
            self.tree.save().expect("Unable to update tree state");
        }
        let mut next_block_number_to_seal_in_tree = self.get_current_rocksdb_block_number();

        let current_db_block = storage.blocks_dal().get_sealed_block_number() + 1;
        let last_block_number_with_metadata =
            storage.blocks_dal().get_last_block_number_with_metadata() + 1;
        drop(storage);

        vlog::info!(
            "initialized metadata calculator. Current rocksDB block: {}. Current Postgres block: {}",
            next_block_number_to_seal_in_tree,
            current_db_block
        );
        metrics::gauge!(
            "server.metadata_calculator.backup_lag",
            (last_block_number_with_metadata - *next_block_number_to_seal_in_tree).0 as f64,
        );

        loop {
            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, metadata_calculator is shutting down");
                break;
            }

            let query_started_at = Instant::now();

            let mut storage = pool.access_storage().await;

            match self.mode {
                MetadataCalculatorMode::Full => {
                    let last_sealed_block = storage.blocks_dal().get_sealed_block_number();
                    let new_blocks: Vec<_> = (next_block_number_to_seal_in_tree.0
                        ..=last_sealed_block.0)
                        .take(self.config.max_block_batch)
                        .flat_map(|block_number| {
                            get_logs_for_l1_batch(&mut storage, L1BatchNumber(block_number))
                        })
                        .collect();

                    metrics::histogram!(
                        "server.metadata_calculator.update_tree.latency.stage",
                        query_started_at.elapsed(),
                        "stage" => "load_changes"
                    );

                    if new_blocks.is_empty() {
                        // we don't have any new data to process. Waiting...
                        #[cfg(not(test))]
                        tokio::time::sleep(self.delay_interval).await;

                        #[cfg(test)]
                        return;
                    } else {
                        next_block_number_to_seal_in_tree =
                            new_blocks.last().unwrap().header.number + 1;

                        self.process_multiple_blocks(&mut storage, new_blocks).await;
                    }
                }
                MetadataCalculatorMode::Lightweight => {
                    let new_block_logs =
                        get_logs_for_l1_batch(&mut storage, next_block_number_to_seal_in_tree);

                    metrics::histogram!(
                        "server.metadata_calculator.update_tree.latency.stage",
                        query_started_at.elapsed(),
                        "stage" => "load_changes"
                    );

                    match new_block_logs {
                        None => {
                            // we don't have any new data to process. Waiting...
                            #[cfg(not(test))]
                            tokio::time::sleep(self.delay_interval).await;

                            #[cfg(test)]
                            return;
                        }
                        Some(block) => {
                            next_block_number_to_seal_in_tree = block.header.number + 1;

                            self.process_block(&mut storage, block).await;
                        }
                    }
                }
                MetadataCalculatorMode::Backup => {
                    unreachable!("Backup mode is disabled");
                }
            }
        }
    }

    fn rocksdb_path(config: &DBConfig, mode: MetadataCalculatorMode) -> &str {
        match mode {
            MetadataCalculatorMode::Full => config.path(),
            _ => config.merkle_tree_fast_ssd_path(),
        }
    }

    pub(crate) fn get_current_rocksdb_block_number(&mut self) -> L1BatchNumber {
        L1BatchNumber(self.tree.block_number())
    }

    // Applies the sealed block to the tree and returns the new root hash
    #[tracing::instrument(skip(self, storage, block))]
    async fn process_block(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        block: WitnessBlockWithLogs,
    ) -> H256 {
        let start = Instant::now();
        let mut start_stage = Instant::now();

        assert_eq!(self.mode, MetadataCalculatorMode::Lightweight);

        let storage_logs = get_filtered_storage_logs(&block.storage_logs, self.mode);
        let total_logs: usize = storage_logs.len();
        let previous_root_hash = self.tree.root_hash();
        let metadata_at_block = self.tree.process_block(storage_logs);

        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            start_stage.elapsed(),
            "stage" => "compute"
        );
        start_stage = Instant::now();

        let metadata = Self::build_block_metadata(&metadata_at_block, &block.header);
        let root_hash = metadata.root_hash;

        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            start_stage.elapsed(),
            "stage" => "prepare_results"
        );

        let block_with_metadata =
            Self::reestimate_block_commit_gas(storage, block.header, metadata);

        start_stage = Instant::now();

        // for consistency it's important to save to postgres before rocksDB
        storage.blocks_dal().save_blocks_metadata(
            block_with_metadata.header.number,
            block_with_metadata.metadata,
            H256::from_slice(&previous_root_hash),
        );

        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            start_stage.elapsed(),
            "stage" => "save_postgres"
        );

        start_stage = Instant::now();

        self.tree.save().expect("Unable to update tree state");

        // only metrics after this point
        self.update_metrics(
            &[block_with_metadata.header],
            total_logs,
            start_stage,
            start,
        );
        root_hash
    }

    #[tracing::instrument(skip(self, storage, blocks))]
    async fn process_multiple_blocks(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        blocks: Vec<WitnessBlockWithLogs>,
    ) -> H256 {
        let start = Instant::now();

        assert_eq!(
            self.mode,
            MetadataCalculatorMode::Full,
            "Lightweight tree shouldn't process multiple blocks"
        );

        let mut start_stage = Instant::now();

        let total_logs: usize = blocks.iter().map(|block| block.storage_logs.len()).sum();
        let storage_logs = blocks.iter().map(|block| block.storage_logs.iter());
        let previous_root_hash = self.tree.root_hash();
        let metadata = self.tree.process_blocks(storage_logs);

        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            start_stage.elapsed(),
            "stage" => "compute"
        );

        let root_hashes: Vec<_> = std::iter::once(&previous_root_hash)
            .chain(metadata.iter().map(|metadata| &metadata.root_hash))
            .map(|hash| H256::from_slice(hash))
            .collect();
        let last_root_hash = *root_hashes.last().unwrap();
        let mut block_headers = Vec::with_capacity(blocks.len());

        for ((metadata_at_block, block), previous_root_hash) in
            metadata.into_iter().zip(blocks).zip(root_hashes)
        {
            start_stage = Instant::now();

            let metadata = Self::build_block_metadata(&metadata_at_block, &block.header);

            metrics::histogram!(
                "server.metadata_calculator.update_tree.latency.stage",
                start_stage.elapsed(),
                "stage" => "prepare_results"
            );

            let block_with_metadata =
                Self::reestimate_block_commit_gas(storage, block.header, metadata);

            start_stage = Instant::now();

            // Save witness input only when running in Full mode.
            self.object_store
                .put(
                    WITNESS_INPUT_BUCKET_PATH,
                    merkle_tree_paths_blob_url(block_with_metadata.header.number),
                    metadata_at_block.witness_input,
                )
                .unwrap();

            metrics::histogram!(
                "server.metadata_calculator.update_tree.latency.stage",
                start_stage.elapsed(),
                "stage" => "save_gcs"
            );

            start_stage = Instant::now();

            // Save the metadata in case the lightweight tree is behind / not running
            storage.blocks_dal().save_blocks_metadata(
                block_with_metadata.header.number,
                block_with_metadata.metadata,
                previous_root_hash,
            );

            storage
                .witness_generator_dal()
                .save_witness_inputs(block_with_metadata.header.number);

            metrics::histogram!(
                "server.metadata_calculator.update_tree.latency.stage",
                start_stage.elapsed(),
                "stage" => "save_postgres"
            );

            block_headers.push(block_with_metadata.header);
        }
        start_stage = Instant::now();

        self.tree.save().expect("Unable to update tree state");

        // only metrics after this point
        self.update_metrics(&block_headers, total_logs, start_stage, start);

        last_root_hash
    }

    /// This is used to improve L1 gas estimation for the commit operation. The estimations are computed
    /// in the State Keeper, where storage writes aren't yet deduplicated, whereas block metadata
    /// contains deduplicated storage writes.
    fn reestimate_block_commit_gas(
        storage: &mut StorageProcessor<'_>,
        block_header: L1BatchHeader,
        metadata: BlockMetadata,
    ) -> BlockWithMetadata {
        let start_stage = Instant::now();
        let unsorted_factory_deps = storage
            .blocks_dal()
            .get_l1_batch_factory_deps(block_header.number);
        let block_with_metadata =
            BlockWithMetadata::new(block_header, metadata, unsorted_factory_deps);
        let commit_gas_cost = crate::gas_tracker::commit_gas_count_for_block(&block_with_metadata);
        storage
            .blocks_dal()
            .update_predicted_block_commit_gas(block_with_metadata.header.number, commit_gas_cost);
        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            start_stage.elapsed(),
            "stage" => "reestimate_block_commit_gas_cost"
        );
        block_with_metadata
    }

    fn update_metrics(
        &self,
        block_headers: &[L1BatchHeader],
        total_logs: usize,
        start_stage: Instant,
        start: Instant,
    ) {
        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            start_stage.elapsed(),
            "stage" => "save_rocksdb"
        );
        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency",
            start.elapsed()
        );

        if total_logs > 0 {
            metrics::histogram!(
                "server.metadata_calculator.update_tree.per_log.latency",
                start.elapsed().div_f32(total_logs as f32)
            );
        }

        let total_tx: usize = block_headers.iter().map(|block| block.tx_count()).sum();
        let total_l1_tx: u16 = block_headers.iter().map(|block| block.l1_tx_count).sum();
        metrics::counter!("server.processed_txs", total_tx as u64, "stage" => "tree");
        metrics::counter!("server.processed_l1_txs", total_l1_tx as u64, "stage" => "tree");
        metrics::histogram!("server.metadata_calculator.log_batch", total_logs as f64);
        metrics::histogram!(
            "server.metadata_calculator.blocks_batch",
            block_headers.len() as f64
        );

        let last_block_number = block_headers.last().unwrap().number.0;
        vlog::info!("block {:?} processed in tree", last_block_number);
        metrics::gauge!(
            "server.block_number",
            last_block_number as f64,
            "stage" => format!("tree_{:?}_mode", self.mode).to_lowercase()
        );
        metrics::histogram!(
            "server.block_latency",
            (seconds_since_epoch() - block_headers.first().unwrap().timestamp) as f64,
            "stage" => format!("tree_{:?}_mode", self.mode).to_lowercase()
        );
    }

    fn build_block_metadata(
        tree_metadata_at_block: &TreeMetadata,
        l1_batch_header: &L1BatchHeader,
    ) -> BlockMetadata {
        let merkle_root_hash = H256::from_slice(&tree_metadata_at_block.root_hash);

        let block_commitment = BlockCommitment::new(
            l1_batch_header.l2_to_l1_logs.clone(),
            tree_metadata_at_block.rollup_last_leaf_index,
            merkle_root_hash,
            tree_metadata_at_block.initial_writes.clone(),
            tree_metadata_at_block.repeated_writes.clone(),
            l1_batch_header.base_system_contracts_hashes.bootloader,
            l1_batch_header.base_system_contracts_hashes.default_aa,
        );
        let block_commitment_hash = block_commitment.hash();
        vlog::trace!("Block commitment {:?}", &block_commitment);

        let metadata = BlockMetadata {
            root_hash: merkle_root_hash,
            rollup_last_leaf_index: tree_metadata_at_block.rollup_last_leaf_index,
            merkle_root_hash,
            initial_writes_compressed: block_commitment.initial_writes_compressed().to_vec(),
            repeated_writes_compressed: block_commitment.repeated_writes_compressed().to_vec(),
            commitment: block_commitment_hash.commitment,
            l2_l1_messages_compressed: block_commitment.l2_l1_logs_compressed().to_vec(),
            l2_l1_merkle_root: block_commitment.l2_l1_logs_merkle_root(),
            block_meta_params: block_commitment.meta_parameters(),
            aux_data_hash: block_commitment_hash.aux_output,
            meta_parameters_hash: block_commitment_hash.meta_parameters,
            pass_through_data_hash: block_commitment_hash.pass_through_data,
        };

        vlog::trace!("Block metadata {:?}", metadata);
        metadata
    }

    /// Encodes storage key using the pre-defined zkSync hasher.
    pub fn key_hash_fn(key: &StorageKey) -> Vec<u8> {
        key.hashed_key().to_fixed_bytes().to_vec()
    }

    fn restore_from_backup(db_config: &DBConfig) {
        let mut engine = BackupEngine::open(
            &BackupEngineOptions::default(),
            db_config.merkle_tree_backup_path(),
        )
        .expect("failed to initialize restore engine");
        if let Err(err) = engine.restore_from_latest_backup(
            db_config.path(),
            db_config.path(),
            &RestoreOptions::default(),
        ) {
            vlog::warn!("can't restore tree from backup {:?}", err);
        }
    }

    fn _backup(&mut self) {
        let started_at = Instant::now();
        let mut engine = BackupEngine::open(
            &BackupEngineOptions::default(),
            self.config.merkle_tree_backup_path(),
        )
        .expect("failed to create backup engine");
        let rocksdb_path = Self::rocksdb_path(&self.config, self.mode);
        let db = DB::open_for_read_only(&Options::default(), rocksdb_path, false)
            .expect("failed to open db for backup");
        engine.create_new_backup(&db).unwrap();
        engine
            .purge_old_backups(self.config.backup_count())
            .expect("failed to purge old backups");
        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            started_at.elapsed(),
            "stage" => "backup_tree"
        );
    }
}

/// Filters the storage log based on the MetadataCalculatorMode and StorageLogKind.
/// | MetadataCalculatorMode       |   Processing           |
/// |------------------------------|------------------------|
/// |    Full                      | Read + Write           |
/// |    Lightweight               |        Write           |
/// |    Backup                    |        Write           |
fn get_filtered_storage_logs(
    storage_logs: &[WitnessStorageLog],
    mode: MetadataCalculatorMode,
) -> Vec<&WitnessStorageLog> {
    storage_logs
        .iter()
        .filter(|log| {
            mode == MetadataCalculatorMode::Full || log.storage_log.kind == StorageLogKind::Write
        })
        .collect()
}

pub(crate) fn get_logs_for_l1_batch(
    storage: &mut StorageProcessor<'_>,
    l1_batch_number: L1BatchNumber,
) -> Option<WitnessBlockWithLogs> {
    let header = storage.blocks_dal().get_block_header(l1_batch_number)?;

    // `BTreeMap` is used because tree needs to process slots in lexicographical order.
    let mut storage_logs: BTreeMap<StorageKey, WitnessStorageLog> = BTreeMap::new();

    let protective_reads = storage
        .storage_logs_dedup_dal()
        .get_protective_reads_for_l1_batch(l1_batch_number);
    let touched_slots = storage
        .storage_logs_dedup_dal()
        .get_touched_slots_for_l1_batch(l1_batch_number);

    let hashed_keys = protective_reads
        .iter()
        .chain(touched_slots.keys())
        .map(|key| key.hashed_key())
        .collect();
    let previous_values = storage
        .storage_logs_dedup_dal()
        .get_previous_storage_values(hashed_keys, l1_batch_number);

    for storage_key in protective_reads {
        let previous_value = previous_values
            .get(&storage_key.hashed_key())
            .cloned()
            .unwrap();

        // Sanity check: value must not change for slots that require protective reads.
        if let Some(value) = touched_slots.get(&storage_key) {
            assert_eq!(
                previous_value, *value,
                "Value was changed for slot that requires protective read"
            );
        }

        storage_logs.insert(
            storage_key,
            WitnessStorageLog {
                storage_log: StorageLog::new_read_log(storage_key, previous_value),
                previous_value,
            },
        );
    }

    for (storage_key, value) in touched_slots {
        let previous_value = previous_values
            .get(&storage_key.hashed_key())
            .cloned()
            .unwrap();

        if previous_value != value {
            storage_logs.insert(
                storage_key,
                WitnessStorageLog {
                    storage_log: StorageLog::new_write_log(storage_key, value),
                    previous_value,
                },
            );
        }
    }

    Some(WitnessBlockWithLogs {
        header,
        storage_logs: storage_logs.into_values().collect(),
    })
}
