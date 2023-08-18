//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use tokio::sync::watch;

use std::time::Duration;

use zksync_config::configs::{
    chain::OperationsManagerConfig,
    database::{DBConfig, MerkleTreeMode},
};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::{HealthUpdater, ReactiveHealthCheck};
use zksync_merkle_tree::domain::TreeMetadata;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{
    block::L1BatchHeader,
    commitment::{L1BatchCommitment, L1BatchMetadata},
};

mod helpers;
mod metrics;
#[cfg(test)]
mod tests;
mod updater;

pub(crate) use self::helpers::L1BatchWithLogs;
use self::{
    helpers::Delayer,
    metrics::{ReportStage, TreeUpdateStage},
    updater::TreeUpdater,
};
use crate::gas_tracker::commit_gas_count_for_l1_batch;

/// Part of [`MetadataCalculator`] related to the operation mode of the Merkle tree.
#[derive(Debug, Clone, Copy)]
pub enum MetadataCalculatorModeConfig<'a> {
    /// In this mode, `MetadataCalculator` computes Merkle tree root hashes and some auxiliary information
    /// for L1 batches, but not witness inputs.
    Lightweight,
    /// In this mode, `MetadataCalculator` will compute witness inputs for all storage operations
    /// and put them into the object store as provided by `store_factory` (e.g., GCS).
    Full {
        store_factory: &'a ObjectStoreFactory,
    },
}

impl MetadataCalculatorModeConfig<'_> {
    fn to_mode(self) -> MerkleTreeMode {
        if matches!(self, Self::Full { .. }) {
            MerkleTreeMode::Full
        } else {
            MerkleTreeMode::Lightweight
        }
    }
}

/// Configuration of [`MetadataCalculator`].
#[derive(Debug)]
pub struct MetadataCalculatorConfig<'a> {
    /// Filesystem path to the RocksDB instance that stores the tree.
    pub db_path: &'a str,
    /// Configuration of the Merkle tree mode.
    pub mode: MetadataCalculatorModeConfig<'a>,
    /// Interval between polling Postgres for updates if no progress was made by the tree.
    pub delay_interval: Duration,
    /// Maximum number of L1 batches to get from Postgres on a single update iteration.
    pub max_l1_batches_per_iter: usize,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    pub multi_get_chunk_size: usize,
    /// Capacity of RocksDB block cache in bytes. Reasonable values range from ~100 MB to several GB.
    pub block_cache_capacity: usize,
}

impl<'a> MetadataCalculatorConfig<'a> {
    pub(crate) fn for_main_node(
        db_config: &'a DBConfig,
        operation_config: &'a OperationsManagerConfig,
        mode: MetadataCalculatorModeConfig<'a>,
    ) -> Self {
        Self {
            db_path: &db_config.merkle_tree.path,
            mode,
            delay_interval: operation_config.delay_interval(),
            max_l1_batches_per_iter: db_config.merkle_tree.max_l1_batches_per_iter,
            multi_get_chunk_size: db_config.merkle_tree.multi_get_chunk_size,
            block_cache_capacity: db_config.merkle_tree.block_cache_size(),
        }
    }
}

#[derive(Debug)]
pub struct MetadataCalculator {
    updater: TreeUpdater,
    delayer: Delayer,
    health_updater: HealthUpdater,
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
        let updater = TreeUpdater::new(mode, config, object_store).await;
        let (_, health_updater) = ReactiveHealthCheck::new("tree");
        Self {
            updater,
            delayer: Delayer::new(config.delay_interval),
            health_updater,
        }
    }

    /// Returns a health check for this calculator.
    pub fn tree_health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    pub async fn run(
        self,
        pool: ConnectionPool,
        prover_pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) {
        let update_task = self.updater.loop_updating_tree(
            self.delayer,
            &pool,
            &prover_pool,
            stop_receiver,
            self.health_updater,
        );
        update_task.await;
    }

    /// This is used to improve L1 gas estimation for the commit operation. The estimations are computed
    /// in the State Keeper, where storage writes aren't yet deduplicated, whereas L1 batch metadata
    /// contains deduplicated storage writes.
    async fn reestimate_l1_batch_commit_gas(
        storage: &mut StorageProcessor<'_>,
        header: &L1BatchHeader,
        metadata: &L1BatchMetadata,
    ) {
        let reestimate_gas_cost = TreeUpdateStage::ReestimateGasCost.start();
        let unsorted_factory_deps = storage
            .blocks_dal()
            .get_l1_batch_factory_deps(header.number)
            .await;
        let commit_gas_cost =
            commit_gas_count_for_l1_batch(header, &unsorted_factory_deps, metadata);
        storage
            .blocks_dal()
            .update_predicted_l1_batch_commit_gas(header.number, commit_gas_cost)
            .await;
        reestimate_gas_cost.report();
    }

    fn build_l1_batch_metadata(
        tree_metadata: TreeMetadata,
        header: &L1BatchHeader,
    ) -> L1BatchMetadata {
        let merkle_root_hash = tree_metadata.root_hash;

        let commitment = L1BatchCommitment::new(
            header.l2_to_l1_logs.clone(),
            tree_metadata.rollup_last_leaf_index,
            merkle_root_hash,
            tree_metadata.initial_writes,
            tree_metadata.repeated_writes,
            header.base_system_contracts_hashes.bootloader,
            header.base_system_contracts_hashes.default_aa,
        );
        let commitment_hash = commitment.hash();
        vlog::trace!("L1 batch commitment: {commitment:?}");

        let metadata = L1BatchMetadata {
            root_hash: merkle_root_hash,
            rollup_last_leaf_index: tree_metadata.rollup_last_leaf_index,
            merkle_root_hash,
            initial_writes_compressed: commitment.initial_writes_compressed().to_vec(),
            repeated_writes_compressed: commitment.repeated_writes_compressed().to_vec(),
            commitment: commitment_hash.commitment,
            l2_l1_messages_compressed: commitment.l2_l1_logs_compressed().to_vec(),
            l2_l1_merkle_root: commitment.l2_l1_logs_merkle_root(),
            block_meta_params: commitment.meta_parameters(),
            aux_data_hash: commitment_hash.aux_output,
            meta_parameters_hash: commitment_hash.meta_parameters,
            pass_through_data_hash: commitment_hash.pass_through_data,
        };

        vlog::trace!("L1 batch metadata: {metadata:?}");
        metadata
    }
}
