//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use tokio::sync::watch;

use std::time::Duration;

use zksync_config::configs::{
    chain::OperationsManagerConfig,
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::{HealthUpdater, ReactiveHealthCheck};
use zksync_merkle_tree::domain::TreeMetadata;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{
    block::L1BatchHeader,
    commitment::{L1BatchCommitment, L1BatchMetadata},
    H256,
};

mod helpers;
mod metrics;
#[cfg(test)]
pub(crate) mod tests;
mod updater;

pub(crate) use self::helpers::{AsyncTreeReader, L1BatchWithLogs, MerkleTreeInfo};
use self::{
    helpers::Delayer,
    metrics::{TreeUpdateStage, METRICS},
    updater::TreeUpdater,
};
use crate::gas_tracker::commit_gas_count_for_l1_batch;

/// Part of [`MetadataCalculator`] related to the operation mode of the Merkle tree.
#[derive(Debug, Clone, Copy)]
pub enum MetadataCalculatorModeConfig<'a> {
    /// In this mode, `MetadataCalculator` computes Merkle tree root hashes and some auxiliary information
    /// for L1 batches, but not witness inputs.
    Lightweight,
    /// In this mode, `MetadataCalculator` will compute commitments and witness inputs for all storage operations
    /// and optionally put witness inputs into the object store as provided by `store_factory` (e.g., GCS).
    Full {
        store_factory: Option<&'a ObjectStoreFactory>,
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
    /// Capacity of RocksDB block cache in bytes. Reasonable values range from ~100 MiB to several GB.
    pub block_cache_capacity: usize,
    /// Capacity of RocksDB memtables. Can be set to a reasonably large value (order of 512 MiB)
    /// to mitigate write stalls.
    pub memtable_capacity: usize,
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    pub stalled_writes_timeout: Duration,
}

impl<'a> MetadataCalculatorConfig<'a> {
    pub(crate) fn for_main_node(
        merkle_tree_config: &'a MerkleTreeConfig,
        operation_config: &'a OperationsManagerConfig,
        mode: MetadataCalculatorModeConfig<'a>,
    ) -> Self {
        Self {
            db_path: &merkle_tree_config.path,
            mode,
            delay_interval: operation_config.delay_interval(),
            max_l1_batches_per_iter: merkle_tree_config.max_l1_batches_per_iter,
            multi_get_chunk_size: merkle_tree_config.multi_get_chunk_size,
            block_cache_capacity: merkle_tree_config.block_cache_size(),
            memtable_capacity: merkle_tree_config.memtable_capacity(),
            stalled_writes_timeout: merkle_tree_config.stalled_writes_timeout(),
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
        // TODO (SMA-1726): restore the tree from backup if appropriate

        let mode = config.mode.to_mode();
        let object_store = match config.mode {
            MetadataCalculatorModeConfig::Full { store_factory } => match store_factory {
                Some(f) => Some(f.create_store().await),
                None => None,
            },
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

    /// Returns a reference to the tree reader.
    pub(crate) fn tree_reader(&self) -> AsyncTreeReader {
        self.updater.tree().reader()
    }

    pub async fn run(
        self,
        pool: ConnectionPool,
        prover_pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        self.updater
            .loop_updating_tree(
                self.delayer,
                &pool,
                &prover_pool,
                stop_receiver,
                self.health_updater,
            )
            .await
    }

    /// This is used to improve L1 gas estimation for the commit operation. The estimations are computed
    /// in the State Keeper, where storage writes aren't yet deduplicated, whereas L1 batch metadata
    /// contains deduplicated storage writes.
    async fn reestimate_l1_batch_commit_gas(
        storage: &mut StorageProcessor<'_>,
        header: &L1BatchHeader,
        metadata: &L1BatchMetadata,
    ) {
        let estimate_latency = METRICS.start_stage(TreeUpdateStage::ReestimateGasCost);
        let unsorted_factory_deps = storage
            .blocks_dal()
            .get_l1_batch_factory_deps(header.number)
            .await
            .unwrap();
        let commit_gas_cost =
            commit_gas_count_for_l1_batch(header, &unsorted_factory_deps, metadata);
        storage
            .blocks_dal()
            .update_predicted_l1_batch_commit_gas(header.number, commit_gas_cost)
            .await
            .unwrap();
        estimate_latency.observe();
    }

    fn build_l1_batch_metadata(
        tree_metadata: TreeMetadata,
        header: &L1BatchHeader,
        events_queue_commitment: Option<H256>,
        bootloader_initial_content_commitment: Option<H256>,
    ) -> L1BatchMetadata {
        let is_pre_boojum = header
            .protocol_version
            .map(|v| v.is_pre_boojum())
            .unwrap_or(true);

        let merkle_root_hash = tree_metadata.root_hash;

        let commitment = L1BatchCommitment::new(
            header.l2_to_l1_logs.clone(),
            tree_metadata.rollup_last_leaf_index,
            merkle_root_hash,
            tree_metadata.initial_writes,
            tree_metadata.repeated_writes,
            header.base_system_contracts_hashes.bootloader,
            header.base_system_contracts_hashes.default_aa,
            header.system_logs.clone(),
            tree_metadata.state_diffs,
            bootloader_initial_content_commitment.unwrap_or_default(),
            events_queue_commitment.unwrap_or_default(),
            is_pre_boojum,
        );
        let commitment_hash = commitment.hash();
        tracing::trace!("L1 batch commitment: {commitment:?}");

        let l2_l1_messages_compressed = if is_pre_boojum {
            commitment.l2_l1_logs_compressed().to_vec()
        } else {
            commitment.system_logs_compressed().to_vec()
        };
        let metadata = L1BatchMetadata {
            root_hash: merkle_root_hash,
            rollup_last_leaf_index: tree_metadata.rollup_last_leaf_index,
            merkle_root_hash,
            initial_writes_compressed: commitment.initial_writes_compressed().to_vec(),
            repeated_writes_compressed: commitment.repeated_writes_compressed().to_vec(),
            commitment: commitment_hash.commitment,
            l2_l1_messages_compressed,
            l2_l1_merkle_root: commitment.l2_l1_logs_merkle_root(),
            block_meta_params: commitment.meta_parameters(),
            aux_data_hash: commitment_hash.aux_output,
            meta_parameters_hash: commitment_hash.meta_parameters,
            pass_through_data_hash: commitment_hash.pass_through_data,
            state_diffs_compressed: commitment.state_diffs_compressed().to_vec(),
            events_queue_commitment,
            bootloader_initial_content_commitment,
        };

        tracing::trace!("L1 batch metadata: {metadata:?}");
        metadata
    }

    // TODO (SMA-1726): Integrate tree backup mode
}
