//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use tokio::sync::watch;

use std::time::Duration;

use zksync_config::configs::chain::OperationsManagerConfig;
use zksync_config::DBConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_merkle_tree::domain::TreeMetadata;
use zksync_object_store::ObjectStoreFactory;
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
enum MetadataCalculatorMode {
    Full,
    Lightweight,
}

impl MetadataCalculatorMode {
    fn as_tag(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Lightweight => "lightweight",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MetadataCalculatorStatus {
    Ready,
    NotReady,
}

/// Part of [`MetadataCalculator`] related to its syncing mode.
#[derive(Debug, Clone, Copy)]
pub enum MetadataCalculatorModeConfig<'a> {
    /// In this mode, `MetadataCalculator` computes Merkle tree root hashes and some auxiliary information
    /// for blocks, but not witness inputs.
    Lightweight,
    /// In this mode, `MetadataCalculator` will compute witness inputs for all storage operations
    /// and put them into the object store as provided by `store_factory` (e.g., GCS).
    Full {
        store_factory: &'a ObjectStoreFactory,
    },
}

impl MetadataCalculatorModeConfig<'_> {
    fn to_mode(self) -> MetadataCalculatorMode {
        if matches!(self, Self::Full { .. }) {
            MetadataCalculatorMode::Full
        } else {
            MetadataCalculatorMode::Lightweight
        }
    }
}

/// Configuration of [`MetadataCalculator`].
#[derive(Debug)]
pub struct MetadataCalculatorConfig<'a> {
    /// Filesystem path to the RocksDB instance that stores the tree.
    pub db_path: &'a str,
    /// Tree syncing mode.
    pub mode: MetadataCalculatorModeConfig<'a>,
    /// Interval between polling Postgres for updates if no progress was made by the tree.
    pub delay_interval: Duration,
    /// Maximum number of L1 batches to get from Postgres on a single update iteration.
    pub max_block_batch: usize,
    /// Sleep interval between tree updates if the tree has made progress. This is only applied
    /// to the tree in the lightweight mode.
    pub throttle_interval: Duration,
}

impl<'a> MetadataCalculatorConfig<'a> {
    pub(crate) fn for_main_node(
        db_config: &'a DBConfig,
        operation_config: &'a OperationsManagerConfig,
        mode: MetadataCalculatorModeConfig<'a>,
    ) -> Self {
        Self {
            db_path: &db_config.new_merkle_tree_ssd_path,
            mode,
            delay_interval: operation_config.delay_interval(),
            throttle_interval: db_config.new_merkle_tree_throttle_interval(),
            max_block_batch: db_config.max_block_batch(),
        }
    }
}

#[derive(Debug)]
pub struct MetadataCalculator {
    updater: TreeUpdater,
    delayer: Delayer,
    throttler: Delayer,
    status_sender: watch::Sender<MetadataCalculatorStatus>,
}

impl MetadataCalculator {
    /// Creates a calculator with the specified `config`.
    pub async fn new(config: &MetadataCalculatorConfig<'_>) -> Self {

        let mode = config.mode.to_mode();
        let object_store = match config.mode {
            MetadataCalculatorModeConfig::Full { store_factory } => {
                Some(store_factory.create_store().await)
            }
            MetadataCalculatorModeConfig::Lightweight => None,
        };
        let updater = TreeUpdater::new(mode, config.db_path, config.max_block_batch, object_store);
        let throttle_interval = if matches!(mode, MetadataCalculatorMode::Lightweight) {
            config.throttle_interval
        } else {
            Duration::ZERO
        };
        let (status_sender, _) = watch::channel(MetadataCalculatorStatus::NotReady);
        Self {
            updater,
            delayer: Delayer::new(config.delay_interval),
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

    pub async fn run(
        self,
        pool: ConnectionPool,
        prover_pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) {
        let update_task = self.updater.loop_updating_tree(
            self.delayer,
            self.throttler,
            &pool,
            &prover_pool,
            stop_receiver,
            self.status_sender,
        );
        update_task.await;
    }

    /// This is used to improve L1 gas estimation for the commit operation. The estimations are computed
    /// in the State Keeper, where storage writes aren't yet deduplicated, whereas block metadata
    /// contains deduplicated storage writes.
    async fn reestimate_block_commit_gas(
        storage: &mut StorageProcessor<'_>,
        block_header: L1BatchHeader,
        metadata: BlockMetadata,
    ) -> BlockWithMetadata {
        let reestimate_gas_cost = TreeUpdateStage::ReestimateGasCost.start();
        let unsorted_factory_deps = storage
            .blocks_dal()
            .get_l1_batch_factory_deps(block_header.number)
            .await;
        let block_with_metadata =
            BlockWithMetadata::new(block_header, metadata, unsorted_factory_deps);
        let commit_gas_cost = crate::gas_tracker::commit_gas_count_for_block(&block_with_metadata);
        storage
            .blocks_dal()
            .update_predicted_block_commit_gas(block_with_metadata.header.number, commit_gas_cost)
            .await;
        reestimate_gas_cost.report();
        block_with_metadata
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
        vlog::trace!("Block commitment: {block_commitment:?}");

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

        vlog::trace!("Block metadata: {metadata:?}");
        metadata
    }
}
