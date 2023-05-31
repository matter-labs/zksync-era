//! Tree updater trait and its implementations.

use tokio::sync::watch;

use std::time::Instant;

use zksync_config::DBConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_merkle_tree::ZkSyncTree as OldTree;
use zksync_merkle_tree2::domain::ZkSyncTree as NewTree;
use zksync_object_store::ObjectStore;
use zksync_storage::RocksDB;
use zksync_types::{block::WitnessBlockWithLogs, L1BatchNumber};

use super::{
    get_logs_for_l1_batch,
    helpers::{Delayer, ZkSyncTree},
    metrics::TreeUpdateStage,
    MetadataCalculator, MetadataCalculatorMode, MetadataCalculatorStatus, TreeImplementation,
};

#[derive(Debug)]
pub(super) struct TreeUpdater {
    mode: MetadataCalculatorMode,
    tree: ZkSyncTree,
    max_block_batch: usize,
    object_store: Option<Box<dyn ObjectStore>>,
}

impl TreeUpdater {
    pub fn new(
        mode: MetadataCalculatorMode,
        db: RocksDB,
        config: &DBConfig,
        object_store: Option<Box<dyn ObjectStore>>,
    ) -> Self {
        use self::TreeImplementation::{New, Old};

        let tree = match mode {
            MetadataCalculatorMode::Full(Old) => ZkSyncTree::Old(OldTree::new(db)),
            MetadataCalculatorMode::Full(New) => ZkSyncTree::New(NewTree::new(db)),
            MetadataCalculatorMode::Lightweight(Old) => {
                ZkSyncTree::Old(OldTree::new_lightweight(db))
            }
            MetadataCalculatorMode::Lightweight(New) => {
                ZkSyncTree::New(NewTree::new_lightweight(db))
            }
        };

        let max_block_batch = if matches!(mode, MetadataCalculatorMode::Lightweight(Old)) {
            // The old tree implementation does not support processing multiple blocks
            // in the lightweight mode.
            1
        } else {
            config.max_block_batch
        };
        Self {
            mode,
            tree,
            max_block_batch,
            object_store,
        }
    }

    #[cfg(test)]
    pub fn tree(&self) -> &ZkSyncTree {
        &self.tree
    }

    pub fn mode(&self) -> MetadataCalculatorMode {
        self.mode
    }

    #[tracing::instrument(skip(self, storage, blocks))]
    fn process_multiple_blocks(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        blocks: Vec<WitnessBlockWithLogs>,
    ) {
        let start = Instant::now();

        let compute_latency = TreeUpdateStage::Compute.start();
        let total_logs: usize = blocks.iter().map(|block| block.storage_logs.len()).sum();
        let storage_logs = blocks.iter().map(|block| block.storage_logs.as_slice());

        let mut previous_root_hash = self.tree.root_hash();
        let metadata = self.tree.process_blocks(storage_logs);
        compute_latency.report();

        let mut block_headers = Vec::with_capacity(blocks.len());
        for (mut metadata_at_block, block) in metadata.into_iter().zip(blocks) {
            let prepare_results_latency = TreeUpdateStage::PrepareResults.start();
            let witness_input = metadata_at_block.witness_input.take();

            let next_root_hash = metadata_at_block.root_hash;
            let metadata =
                MetadataCalculator::build_block_metadata(metadata_at_block, &block.header);
            prepare_results_latency.report();

            let block_with_metadata =
                MetadataCalculator::reestimate_block_commit_gas(storage, block.header, metadata);
            let block_number = block_with_metadata.header.number;

            let object_key = self.object_store.as_ref().map(|object_store| {
                let witness_input =
                    witness_input.expect("No witness input provided by tree; this is a bug");

                TreeUpdateStage::SaveWitnesses
                    .run(|| object_store.put(block_number, &witness_input).unwrap())
            });

            // Save the metadata in case the lightweight tree is behind / not running
            let metadata = block_with_metadata.metadata;
            TreeUpdateStage::SavePostgres.run(|| {
                storage.blocks_dal().save_blocks_metadata(
                    block_number,
                    metadata,
                    previous_root_hash,
                );
                // ^ Note that `save_blocks_metadata()` will not blindly overwrite changes if the block
                // metadata already exists; instead, it'll check that the old an new metadata match.
                // That is, if we run both tree implementations, we'll get metadata correspondence
                // right away without having to implement dedicated code.

                if let Some(object_key) = &object_key {
                    storage
                        .witness_generator_dal()
                        .save_witness_inputs(block_number, object_key);
                }
            });

            previous_root_hash = next_root_hash;
            block_headers.push(block_with_metadata.header);
        }

        TreeUpdateStage::SaveRocksDB.run(|| self.tree.save());
        MetadataCalculator::update_metrics(self.mode, &block_headers, total_logs, start);
    }

    fn tree_implementation(&self) -> TreeImplementation {
        match &self.tree {
            ZkSyncTree::Old(_) => TreeImplementation::Old,
            ZkSyncTree::New(_) => TreeImplementation::New,
        }
    }

    fn step(&mut self, mut storage: StorageProcessor<'_>, next_block_to_seal: &mut L1BatchNumber) {
        let new_blocks: Vec<_> = TreeUpdateStage::LoadChanges.run(|| {
            let last_sealed_block = storage.blocks_dal().get_sealed_block_number();
            (next_block_to_seal.0..=last_sealed_block.0)
                .map(L1BatchNumber)
                .take(self.max_block_batch)
                .flat_map(|block_number| get_logs_for_l1_batch(&mut storage, block_number))
                .collect()
        });

        if let Some(last_block) = new_blocks.last() {
            *next_block_to_seal = last_block.header.number + 1;
            self.process_multiple_blocks(&mut storage, new_blocks);
        }
    }

    /// The processing loop for this updater.
    pub fn loop_updating_tree(
        mut self,
        delayer: Delayer,
        throttler: Delayer,
        pool: &ConnectionPool,
        mut stop_receiver: watch::Receiver<bool>,
        status_sender: watch::Sender<MetadataCalculatorStatus>,
    ) {
        let mut storage = pool.access_storage_blocking();

        // Ensure genesis creation
        let tree = &mut self.tree;
        if tree.is_empty() {
            let Some(logs) = get_logs_for_l1_batch(&mut storage, L1BatchNumber(0)) else {
                panic!("Missing storage logs for the genesis block");
            };
            tree.process_block(&logs.storage_logs);
            tree.save();
        }
        let mut next_block_to_seal = L1BatchNumber(tree.block_number());

        let current_db_block = storage.blocks_dal().get_sealed_block_number() + 1;
        let last_block_number_with_metadata =
            storage.blocks_dal().get_last_block_number_with_metadata() + 1;
        drop(storage);

        let tree_tag = self.tree_implementation().as_tag();
        vlog::info!(
            "Initialized metadata calculator with {} tree implementation. \
             Current RocksDB block: {}. Current Postgres block: {}",
            tree_tag,
            next_block_to_seal,
            current_db_block
        );
        metrics::gauge!(
            "server.metadata_calculator.backup_lag",
            (last_block_number_with_metadata - *next_block_to_seal).0 as f64,
            "tree" => tree_tag
        );
        status_sender.send_replace(MetadataCalculatorStatus::Ready);

        loop {
            if *stop_receiver.borrow_and_update() {
                vlog::info!("Stop signal received, metadata_calculator is shutting down");
                break;
            }
            let storage = pool.access_storage_blocking();

            let next_block_snapshot = *next_block_to_seal;
            self.step(storage, &mut next_block_to_seal);
            if next_block_snapshot == *next_block_to_seal {
                // We didn't make any progress.
                delayer.wait(&self.tree);
            } else {
                // We've made some progress; apply throttling if necessary.
                throttler.wait(&self.tree);
            }
        }
    }
}
