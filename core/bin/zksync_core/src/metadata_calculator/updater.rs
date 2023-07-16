//! Tree updater trait and its implementations.

use tokio::sync::watch;

use std::time::Instant;

use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_object_store::ObjectStore;
use zksync_storage::{db::NamedColumnFamily, RocksDB};
use zksync_types::{block::WitnessBlockWithLogs, L1BatchNumber};

use super::{
    get_logs_for_l1_batch,
    helpers::{AsyncTree, Delayer},
    metrics::TreeUpdateStage,
    MetadataCalculator, MetadataCalculatorMode, MetadataCalculatorStatus,
};

#[derive(Debug)]
pub(super) struct TreeUpdater {
    mode: MetadataCalculatorMode,
    tree: AsyncTree,
    max_block_batch: usize,
    object_store: Option<Box<dyn ObjectStore>>,
}

impl TreeUpdater {
    pub fn new(
        mode: MetadataCalculatorMode,
        db_path: &str,
        max_block_batch: usize,
        object_store: Option<Box<dyn ObjectStore>>,
    ) -> Self {
        assert!(
            max_block_batch > 0,
            "Maximum block batch is misconfigured to be 0; please update it to positive value"
        );

        let db = Self::create_db(db_path);
        let tree = AsyncTree::new(match mode {
            MetadataCalculatorMode::Full => ZkSyncTree::new(db),
            MetadataCalculatorMode::Lightweight => ZkSyncTree::new_lightweight(db),
        });

        Self {
            mode,
            tree,
            max_block_batch,
            object_store,
        }
    }

    fn create_db<CF: NamedColumnFamily>(path: &str) -> RocksDB<CF> {
        let db = RocksDB::new(path, true);
        if cfg!(test) {
            // We need sync writes for the unit tests to execute reliably. With the default config,
            // some writes to RocksDB may occur, but not be visible to the test code.
            db.with_sync_writes()
        } else {
            db
        }
    }

    #[cfg(test)]
    pub fn tree(&self) -> &AsyncTree {
        &self.tree
    }

    pub fn mode(&self) -> MetadataCalculatorMode {
        self.mode
    }

    async fn process_multiple_blocks(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        prover_storage: &mut StorageProcessor<'_>,
        blocks: Vec<WitnessBlockWithLogs>,
    ) {
        let start = Instant::now();

        let compute_latency = TreeUpdateStage::Compute.start();
        let total_logs: usize = blocks.iter().map(|block| block.storage_logs.len()).sum();
        if let (Some(first), Some(last)) = (blocks.first(), blocks.last()) {
            let l1_batch_numbers = first.header.number.0..=last.header.number.0;
            vlog::info!("Processing L1 batches #{l1_batch_numbers:?} with {total_logs} total logs");
        };

        let (storage_logs, block_headers): (Vec<_>, Vec<_>) = blocks
            .into_iter()
            .map(|block| (block.storage_logs, block.header))
            .unzip();
        let mut previous_root_hash = self.tree.root_hash();
        let metadata = self.tree.process_blocks(storage_logs).await;
        compute_latency.report();

        let mut updated_headers = Vec::with_capacity(block_headers.len());
        for (mut metadata_at_block, block_header) in metadata.into_iter().zip(block_headers) {
            let prepare_results_latency = TreeUpdateStage::PrepareResults.start();
            let witness_input = metadata_at_block.witness.take();

            let next_root_hash = metadata_at_block.root_hash;
            let metadata =
                MetadataCalculator::build_block_metadata(metadata_at_block, &block_header);
            prepare_results_latency.report();

            let block_with_metadata =
                MetadataCalculator::reestimate_block_commit_gas(storage, block_header, metadata)
                    .await;
            let block_number = block_with_metadata.header.number;

            let object_key = if let Some(object_store) = &self.object_store {
                let witness_input =
                    witness_input.expect("No witness input provided by tree; this is a bug");
                let save_witnesses_latency = TreeUpdateStage::SaveWitnesses.start();
                let object_key = object_store
                    .put(block_number, &witness_input)
                    .await
                    .unwrap();
                save_witnesses_latency.report();

                vlog::info!(
                    "Saved witnesses for L1 batch #{block_number} to object storage at `{object_key}`"
                );
                Some(object_key)
            } else {
                None
            };

            // Save the metadata in case the lightweight tree is behind / not running
            let metadata = &block_with_metadata.metadata;
            let save_postgres_latency = TreeUpdateStage::SavePostgres.start();
            storage
                .blocks_dal()
                .save_blocks_metadata(block_number, metadata, previous_root_hash)
                .await;
            // ^ Note that `save_blocks_metadata()` will not blindly overwrite changes if the block
            // metadata already exists; instead, it'll check that the old an new metadata match.
            // That is, if we run both tree implementations, we'll get metadata correspondence
            // right away without having to implement dedicated code.

            if let Some(object_key) = &object_key {
                prover_storage
                    .witness_generator_dal()
                    .save_witness_inputs(block_number, object_key)
                    .await;
                prover_storage
                    .fri_witness_generator_dal()
                    .save_witness_inputs(block_number, object_key)
                    .await;
            }
            save_postgres_latency.report();
            vlog::info!("Updated metadata for L1 batch #{block_number} in Postgres");

            previous_root_hash = next_root_hash;
            updated_headers.push(block_with_metadata.header);
        }

        let save_rocksdb_latency = TreeUpdateStage::SaveRocksDB.start();
        self.tree.save().await;
        save_rocksdb_latency.report();
        MetadataCalculator::update_metrics(self.mode, &updated_headers, total_logs, start);
    }

    async fn step(
        &mut self,
        mut storage: StorageProcessor<'_>,
        mut prover_storage: StorageProcessor<'_>,
        next_block_to_seal: &mut L1BatchNumber,
    ) {
        let load_changes_latency = TreeUpdateStage::LoadChanges.start();
        let last_sealed_block = storage.blocks_dal().get_sealed_block_number().await;
        let last_requested_block = next_block_to_seal.0 + self.max_block_batch as u32 - 1;
        let last_requested_block = last_requested_block.min(last_sealed_block.0);
        let block_numbers = next_block_to_seal.0..=last_requested_block;
        if block_numbers.is_empty() {
            vlog::trace!(
                "No blocks to seal: block numbers range to be loaded {block_numbers:?} is empty"
            );
        } else {
            vlog::info!("Loading blocks with numbers {block_numbers:?} to update Merkle tree");
        }

        let mut new_blocks = vec![];
        for block_number in block_numbers {
            let logs = get_logs_for_l1_batch(&mut storage, L1BatchNumber(block_number)).await;
            new_blocks.extend(logs);
        }
        load_changes_latency.report();

        if let Some(last_block) = new_blocks.last() {
            *next_block_to_seal = last_block.header.number + 1;
            self.process_multiple_blocks(&mut storage, &mut prover_storage, new_blocks)
                .await;
        }
    }

    /// The processing loop for this updater.
    pub async fn loop_updating_tree(
        mut self,
        delayer: Delayer,
        throttler: Delayer,
        pool: &ConnectionPool,
        prover_pool: &ConnectionPool,
        mut stop_receiver: watch::Receiver<bool>,
        status_sender: watch::Sender<MetadataCalculatorStatus>,
    ) {
        let mut storage = pool.access_storage_tagged("metadata_calculator").await;

        // Ensure genesis creation
        let tree = &mut self.tree;
        if tree.is_empty() {
            let Some(logs) = get_logs_for_l1_batch(&mut storage, L1BatchNumber(0)).await else {
                panic!("Missing storage logs for the genesis block");
            };
            tree.process_block(logs.storage_logs).await;
            tree.save().await;
        }
        let mut next_block_to_seal = L1BatchNumber(tree.block_number());

        let current_db_block = storage.blocks_dal().get_sealed_block_number().await + 1;
        let last_block_number_with_metadata = storage
            .blocks_dal()
            .get_last_block_number_with_metadata()
            .await
            + 1;
        drop(storage);

        vlog::info!(
            "Initialized metadata calculator with {max_block_batch} max batch size. \
             Current RocksDB block: {next_block_to_seal}, current Postgres block: {current_db_block}, \
             last block with metadata: {last_block_number_with_metadata}",
            max_block_batch = self.max_block_batch
        );
        metrics::gauge!(
            "server.metadata_calculator.backup_lag",
            (last_block_number_with_metadata - *next_block_to_seal).0 as f64
        );
        status_sender.send_replace(MetadataCalculatorStatus::Ready);

        loop {
            if *stop_receiver.borrow_and_update() {
                vlog::info!("Stop signal received, metadata_calculator is shutting down");
                break;
            }
            let storage = pool.access_storage_tagged("metadata_calculator").await;
            let prover_storage = prover_pool
                .access_storage_tagged("metadata_calculator")
                .await;

            let next_block_snapshot = *next_block_to_seal;
            self.step(storage, prover_storage, &mut next_block_to_seal)
                .await;
            let delay = if next_block_snapshot == *next_block_to_seal {
                vlog::trace!(
                    "Metadata calculator (next L1 batch: #{next_block_to_seal}) \
                     didn't make any progress; delaying it using {delayer:?}"
                );
                delayer.wait(&self.tree)
            } else {
                vlog::trace!(
                    "Metadata calculator (next L1 batch: #{next_block_to_seal}) \
                     made progress from #{next_block_snapshot}; throttling it using {throttler:?}"
                );
                throttler.wait(&self.tree)
            };

            // The delays we're operating with are reasonably small, but selecting between the delay
            // and the stop receiver still allows to be more responsive during shutdown.
            tokio::select! {
                _ = stop_receiver.changed() => {
                    vlog::info!("Stop signal received, metadata_calculator is shutting down");
                    break;
                }
                () = delay => { /* The delay has passed */ }
            }
        }
    }
}
