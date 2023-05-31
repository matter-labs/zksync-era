//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use tokio::sync::watch;

use std::time::Duration;

use zksync_config::{DBConfig, ZkSyncConfig};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_merkle_tree::TreeMetadata;
use zksync_object_store::ObjectStoreFactory;
use zksync_storage::{
    db::Database,
    rocksdb::{
        backup::{BackupEngine, BackupEngineOptions, RestoreOptions},
        Options, DB,
    },
    RocksDB,
};
use zksync_types::{
    block::L1BatchHeader,
    commitment::{BlockCommitment, BlockMetadata, BlockWithMetadata},
};

mod healthcheck;
mod helpers;
mod metrics;
#[cfg(test)]
mod tests;
mod updater;

pub use self::healthcheck::TreeHealthCheck;
pub(crate) use self::helpers::get_logs_for_l1_batch;
use self::{helpers::Delayer, metrics::TreeUpdateStage, updater::TreeUpdater};

#[derive(Debug, Copy, Clone)]
pub enum TreeImplementation {
    Old,
    New,
}

impl TreeImplementation {
    fn as_tag(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum MetadataCalculatorMode {
    Full(TreeImplementation),
    Lightweight(TreeImplementation),
}

impl MetadataCalculatorMode {
    fn as_tag(self) -> &'static str {
        match self {
            Self::Full(TreeImplementation::Old) => "full",
            // ^ chosen for backward compatibility
            Self::Full(TreeImplementation::New) => "full_new",
            Self::Lightweight(TreeImplementation::Old) => "lightweight",
            // ^ chosen for backward compatibility
            Self::Lightweight(TreeImplementation::New) => "lightweight_new",
        }
    }

    fn tree_implementation(self) -> TreeImplementation {
        match self {
            Self::Full(implementation) | Self::Lightweight(implementation) => implementation,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MetadataCalculatorStatus {
    Ready,
    NotReady,
}

#[derive(Debug)]
pub struct MetadataCalculator {
    updater: TreeUpdater,
    delayer: Delayer,
    throttler: Delayer,
    status_sender: watch::Sender<MetadataCalculatorStatus>,
}

impl MetadataCalculator {
    /// Creates a calculator operating in the lightweight sync mode. In this mode, the calculator
    /// computes Merkle tree root hashes and some auxiliary information for blocks, but not
    /// witness inputs.
    pub fn lightweight(config: &ZkSyncConfig, implementation: TreeImplementation) -> Self {
        let mode = MetadataCalculatorMode::Lightweight(implementation);
        Self::new(config, None, mode)
    }

    /// Creates a calculator operating in the full sync mode. In this mode, the calculator
    /// will compute witness inputs for all storage operations and put them into the object store
    /// as provided by `store_factory` (e.g., GCS).
    pub fn full(
        config: &ZkSyncConfig,
        store_factory: &ObjectStoreFactory,
        implementation: TreeImplementation,
    ) -> Self {
        let mode = MetadataCalculatorMode::Full(implementation);
        Self::new(config, Some(store_factory), mode)
    }

    fn new(
        config: &ZkSyncConfig,
        store_factory: Option<&ObjectStoreFactory>,
        mode: MetadataCalculatorMode,
    ) -> Self {
        use self::TreeImplementation::New;

        let db_path = Self::db_path(&config.db, mode);
        let db = Self::create_db(db_path);
        let object_store = store_factory.map(ObjectStoreFactory::create_store);
        let updater = TreeUpdater::new(mode, db, &config.db, object_store);
        let delay_interval = config.chain.operations_manager.delay_interval();
        let throttle_interval = if matches!(mode, MetadataCalculatorMode::Lightweight(New)) {
            config.db.new_merkle_tree_throttle_interval()
        } else {
            Duration::ZERO
        };
        let (status_sender, _) = watch::channel(MetadataCalculatorStatus::NotReady);
        Self {
            updater,
            delayer: Delayer::new(delay_interval),
            throttler: Delayer::new(throttle_interval),
            status_sender,
        }
    }

    /// Returns a health check for this calculator.
    pub fn tree_health_check(&self) -> TreeHealthCheck {
        let receiver = self.status_sender.subscribe();
        TreeHealthCheck::new(receiver, self.updater.mode())
    }

    /// Returns the tag for this calculator usable in metrics reporting.
    pub fn tree_tag(&self) -> &'static str {
        self.updater.mode().as_tag()
    }

    fn db_path(config: &DBConfig, mode: MetadataCalculatorMode) -> &str {
        use self::TreeImplementation::{New, Old};

        match mode {
            MetadataCalculatorMode::Full(Old) => config.path(),
            MetadataCalculatorMode::Lightweight(Old) => config.merkle_tree_fast_ssd_path(),
            MetadataCalculatorMode::Full(New) | MetadataCalculatorMode::Lightweight(New) => {
                &config.new_merkle_tree_ssd_path
            }
        }
    }

    fn create_db(path: &str) -> RocksDB {
        let db = RocksDB::new(Database::MerkleTree, path, true);
        if cfg!(test) {
            // We need sync writes for the unit tests to execute reliably. With the default config,
            // some writes to RocksDB may occur, but not be visible to the test code.
            db.with_sync_writes()
        } else {
            db
        }
    }

    pub fn run(self, pool: &ConnectionPool, stop_receiver: watch::Receiver<bool>) {
        self.updater.loop_updating_tree(
            self.delayer,
            self.throttler,
            pool,
            stop_receiver,
            self.status_sender,
        );
    }

    /// This is used to improve L1 gas estimation for the commit operation. The estimations are computed
    /// in the State Keeper, where storage writes aren't yet deduplicated, whereas block metadata
    /// contains deduplicated storage writes.
    fn reestimate_block_commit_gas(
        storage: &mut StorageProcessor<'_>,
        block_header: L1BatchHeader,
        metadata: BlockMetadata,
    ) -> BlockWithMetadata {
        TreeUpdateStage::ReestimateGasCost.run(|| {
            let unsorted_factory_deps = storage
                .blocks_dal()
                .get_l1_batch_factory_deps(block_header.number);
            let block_with_metadata =
                BlockWithMetadata::new(block_header, metadata, unsorted_factory_deps);
            let commit_gas_cost =
                crate::gas_tracker::commit_gas_count_for_block(&block_with_metadata);
            storage.blocks_dal().update_predicted_block_commit_gas(
                block_with_metadata.header.number,
                commit_gas_cost,
            );
            block_with_metadata
        })
    }

    fn build_block_metadata(
        tree_metadata_at_block: TreeMetadata,
        l1_batch_header: &L1BatchHeader,
    ) -> BlockMetadata {
        let merkle_root_hash = tree_metadata_at_block.root_hash;

        let block_commitment = BlockCommitment::new(
            l1_batch_header.l2_to_l1_logs.clone(),
            tree_metadata_at_block.rollup_last_leaf_index,
            merkle_root_hash,
            tree_metadata_at_block.initial_writes,
            tree_metadata_at_block.repeated_writes,
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

    fn _restore_from_backup(db_config: &DBConfig) {
        let backup_path = db_config.merkle_tree_backup_path();
        let mut engine = BackupEngine::open(&BackupEngineOptions::default(), backup_path)
            .expect("failed to initialize restore engine");
        let rocksdb_path = db_config.path();
        if let Err(err) = engine.restore_from_latest_backup(
            rocksdb_path,
            rocksdb_path,
            &RestoreOptions::default(),
        ) {
            vlog::warn!("can't restore tree from backup {:?}", err);
        }
    }

    fn _backup(config: &DBConfig, mode: MetadataCalculatorMode) {
        let backup_latency = TreeUpdateStage::_Backup.start();
        let backup_path = config.merkle_tree_backup_path();
        let mut engine = BackupEngine::open(&BackupEngineOptions::default(), backup_path)
            .expect("failed to create backup engine");
        let rocksdb_path = Self::db_path(config, mode);
        let db = DB::open_for_read_only(&Options::default(), rocksdb_path, false)
            .expect("failed to open db for backup");
        engine.create_new_backup(&db).unwrap();
        engine
            .purge_old_backups(config.backup_count())
            .expect("failed to purge old backups");
        backup_latency.report();
    }
}
