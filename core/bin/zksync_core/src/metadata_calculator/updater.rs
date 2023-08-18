//! Tree updater trait and its implementations.

use futures::{future, FutureExt};
use tokio::sync::watch;

use std::{ops, time::Instant};

use zksync_config::configs::database::MerkleTreeMode;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::HealthUpdater;
use zksync_merkle_tree::domain::TreeMetadata;
use zksync_object_store::ObjectStore;
use zksync_types::{block::L1BatchHeader, writes::InitialStorageWrite, L1BatchNumber, U256};

use super::{
    helpers::{AsyncTree, Delayer, L1BatchWithLogs, TreeHealthCheckDetails},
    metrics::{ReportStage, TreeUpdateStage},
    MetadataCalculator, MetadataCalculatorConfig,
};

#[derive(Debug)]
pub(super) struct TreeUpdater {
    mode: MerkleTreeMode,
    tree: AsyncTree,
    max_l1_batches_per_iter: usize,
    object_store: Option<Box<dyn ObjectStore>>,
}

impl TreeUpdater {
    pub async fn new(
        mode: MerkleTreeMode,
        config: &MetadataCalculatorConfig<'_>,
        object_store: Option<Box<dyn ObjectStore>>,
    ) -> Self {
        assert!(
            config.max_l1_batches_per_iter > 0,
            "Maximum L1 batches per iteration is misconfigured to be 0; please update it to positive value"
        );

        let db_path = config.db_path.into();
        let tree = AsyncTree::new(
            db_path,
            mode,
            config.multi_get_chunk_size,
            config.block_cache_capacity,
        )
        .await;
        Self {
            mode,
            tree,
            max_l1_batches_per_iter: config.max_l1_batches_per_iter,
            object_store,
        }
    }

    #[cfg(test)]
    pub fn tree(&self) -> &AsyncTree {
        &self.tree
    }

    async fn process_l1_batch(
        &mut self,
        l1_batch: L1BatchWithLogs,
    ) -> (L1BatchHeader, TreeMetadata, Option<String>) {
        let compute_latency = TreeUpdateStage::Compute.start();
        let mut metadata = self.tree.process_l1_batch(l1_batch.storage_logs).await;
        compute_latency.report();

        let witness_input = metadata.witness.take();
        let l1_batch_number = l1_batch.header.number;
        let object_key = if let Some(object_store) = &self.object_store {
            let witness_input =
                witness_input.expect("No witness input provided by tree; this is a bug");
            let save_witnesses_latency = TreeUpdateStage::SaveWitnesses.start();
            let object_key = object_store
                .put(l1_batch_number, &witness_input)
                .await
                .unwrap();
            save_witnesses_latency.report();

            vlog::info!(
                "Saved witnesses for L1 batch #{l1_batch_number} to object storage at `{object_key}`"
            );
            Some(object_key)
        } else {
            None
        };

        (l1_batch.header, metadata, object_key)
    }

    /// Processes a range of L1 batches with a single flushing of the tree updates to RocksDB at the end.
    /// This allows to save on RocksDB I/O ops.
    ///
    /// Returns the number of the next L1 batch to be processed by the tree.
    ///
    /// # Implementation details
    ///
    /// We load L1 batch data from Postgres in parallel with updating the tree. (Naturally, we need to load
    /// the first L1 batch data beforehand.) This allows saving some time if we actually process
    /// multiple L1 batches at once (e.g., during the initial tree syncing), and if loading data from Postgres
    /// is slow for whatever reason.
    async fn process_multiple_batches(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        prover_storage: &mut StorageProcessor<'_>,
        l1_batch_numbers: ops::RangeInclusive<u32>,
    ) -> L1BatchNumber {
        let start = Instant::now();
        vlog::info!("Processing L1 batches #{l1_batch_numbers:?}");
        let first_l1_batch_number = L1BatchNumber(*l1_batch_numbers.start());
        let last_l1_batch_number = L1BatchNumber(*l1_batch_numbers.end());
        let mut l1_batch_data = L1BatchWithLogs::new(storage, first_l1_batch_number).await;

        let mut previous_root_hash = self.tree.root_hash();
        let mut total_logs = 0;
        let mut updated_headers = vec![];
        for l1_batch_number in l1_batch_numbers {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            let Some(current_l1_batch_data) = l1_batch_data else {
                return l1_batch_number;
            };
            total_logs += current_l1_batch_data.storage_logs.len();

            let process_l1_batch_task = self.process_l1_batch(current_l1_batch_data);
            let load_next_l1_batch_task = async {
                if l1_batch_number < last_l1_batch_number {
                    L1BatchWithLogs::new(storage, l1_batch_number + 1).await
                } else {
                    None // Don't need to load the next L1 batch after the last one we're processing.
                }
            };
            let ((header, metadata, object_key), next_l1_batch_data) =
                future::join(process_l1_batch_task, load_next_l1_batch_task).await;

            let prepare_results_latency = TreeUpdateStage::PrepareResults.start();
            Self::check_initial_writes_consistency(
                storage,
                header.number,
                &metadata.initial_writes,
            )
            .await;
            let metadata = MetadataCalculator::build_l1_batch_metadata(metadata, &header);
            prepare_results_latency.report();

            MetadataCalculator::reestimate_l1_batch_commit_gas(storage, &header, &metadata).await;

            let save_postgres_latency = TreeUpdateStage::SavePostgres.start();
            storage
                .blocks_dal()
                .save_l1_batch_metadata(l1_batch_number, &metadata, previous_root_hash)
                .await;
            // ^ Note that `save_l1_batch_metadata()` will not blindly overwrite changes if L1 batch
            // metadata already exists; instead, it'll check that the old an new metadata match.
            // That is, if we run multiple tree instances, we'll get metadata correspondence
            // right away without having to implement dedicated code.

            if let Some(object_key) = &object_key {
                prover_storage
                    .witness_generator_dal()
                    .save_witness_inputs(l1_batch_number, object_key)
                    .await;
                prover_storage
                    .fri_witness_generator_dal()
                    .save_witness_inputs(l1_batch_number, object_key)
                    .await;
            }
            save_postgres_latency.report();
            vlog::info!("Updated metadata for L1 batch #{l1_batch_number} in Postgres");

            previous_root_hash = metadata.merkle_root_hash;
            updated_headers.push(header);
            l1_batch_data = next_l1_batch_data;
        }

        let save_rocksdb_latency = TreeUpdateStage::SaveRocksDB.start();
        self.tree.save().await;
        save_rocksdb_latency.report();
        MetadataCalculator::update_metrics(self.mode, &updated_headers, total_logs, start);

        last_l1_batch_number + 1
    }

    async fn step(
        &mut self,
        mut storage: StorageProcessor<'_>,
        mut prover_storage: StorageProcessor<'_>,
        next_l1_batch_to_seal: &mut L1BatchNumber,
    ) {
        let last_sealed_l1_batch = storage.blocks_dal().get_sealed_l1_batch_number().await;
        let last_requested_l1_batch =
            next_l1_batch_to_seal.0 + self.max_l1_batches_per_iter as u32 - 1;
        let last_requested_l1_batch = last_requested_l1_batch.min(last_sealed_l1_batch.0);
        let l1_batch_numbers = next_l1_batch_to_seal.0..=last_requested_l1_batch;
        if l1_batch_numbers.is_empty() {
            vlog::trace!(
                "No L1 batches to seal: batch numbers range to be loaded {l1_batch_numbers:?} is empty"
            );
        } else {
            vlog::info!("Updating Merkle tree with L1 batches #{l1_batch_numbers:?}");
            *next_l1_batch_to_seal = self
                .process_multiple_batches(&mut storage, &mut prover_storage, l1_batch_numbers)
                .await;
        }
    }

    /// The processing loop for this updater.
    pub async fn loop_updating_tree(
        mut self,
        delayer: Delayer,
        pool: &ConnectionPool,
        prover_pool: &ConnectionPool,
        mut stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
    ) {
        let mut storage = pool.access_storage_tagged("metadata_calculator").await;

        // Ensure genesis creation
        let tree = &mut self.tree;
        if tree.is_empty() {
            let Some(logs) = L1BatchWithLogs::new(&mut storage, L1BatchNumber(0)).await else {
                panic!("Missing storage logs for the genesis L1 batch");
            };
            tree.process_l1_batch(logs.storage_logs).await;
            tree.save().await;
        }
        let mut next_l1_batch_to_seal = tree.next_l1_batch_number();

        let current_db_batch = storage.blocks_dal().get_sealed_l1_batch_number().await;
        let last_l1_batch_with_metadata = storage
            .blocks_dal()
            .get_last_l1_batch_number_with_metadata()
            .await;
        drop(storage);

        vlog::info!(
            "Initialized metadata calculator with {max_batches_per_iter} max L1 batches per iteration. \
             Next L1 batch for Merkle tree: {next_l1_batch_to_seal}, current Postgres L1 batch: {current_db_batch}, \
             last L1 batch with metadata: {last_l1_batch_with_metadata}",
            max_batches_per_iter = self.max_l1_batches_per_iter
        );
        let backup_lag =
            (last_l1_batch_with_metadata.0 + 1).saturating_sub(next_l1_batch_to_seal.0);
        metrics::gauge!("server.metadata_calculator.backup_lag", backup_lag as f64);

        let health = TreeHealthCheckDetails {
            mode: self.mode,
            next_l1_batch_to_seal,
        };
        health_updater.update(health.into());

        if next_l1_batch_to_seal > last_l1_batch_with_metadata + 1 {
            // Check stop signal before proceeding with a potentially time-consuming operation.
            if *stop_receiver.borrow_and_update() {
                vlog::info!("Stop signal received, metadata_calculator is shutting down");
                return;
            }

            vlog::warn!(
                "Next L1 batch of the tree ({next_l1_batch_to_seal}) is greater than last L1 batch with metadata in Postgres \
                 ({last_l1_batch_with_metadata}); this may be a result of restoring Postgres from a snapshot. \
                 Truncating Merkle tree versions so that this mismatch is fixed..."
            );
            tree.revert_logs(last_l1_batch_with_metadata);
            tree.save().await;
            next_l1_batch_to_seal = tree.next_l1_batch_number();
            vlog::info!("Truncated Merkle tree to L1 batch #{next_l1_batch_to_seal}");

            let health = TreeHealthCheckDetails {
                mode: self.mode,
                next_l1_batch_to_seal,
            };
            health_updater.update(health.into());
        }

        loop {
            if *stop_receiver.borrow_and_update() {
                vlog::info!("Stop signal received, metadata_calculator is shutting down");
                break;
            }
            let storage = pool.access_storage_tagged("metadata_calculator").await;
            let prover_storage = prover_pool
                .access_storage_tagged("metadata_calculator")
                .await;

            let snapshot = *next_l1_batch_to_seal;
            self.step(storage, prover_storage, &mut next_l1_batch_to_seal)
                .await;
            let delay = if snapshot == *next_l1_batch_to_seal {
                vlog::trace!(
                    "Metadata calculator (next L1 batch: #{next_l1_batch_to_seal}) \
                     didn't make any progress; delaying it using {delayer:?}"
                );
                delayer.wait(&self.tree).left_future()
            } else {
                let health = TreeHealthCheckDetails {
                    mode: self.mode,
                    next_l1_batch_to_seal,
                };
                health_updater.update(health.into());

                vlog::trace!(
                    "Metadata calculator (next L1 batch: #{next_l1_batch_to_seal}) made progress from #{snapshot}"
                );
                future::ready(()).right_future()
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
        drop(health_updater); // Explicitly mark where the updater should be dropped
    }

    async fn check_initial_writes_consistency(
        connection: &mut StorageProcessor<'_>,
        l1_batch_number: L1BatchNumber,
        tree_initial_writes: &[InitialStorageWrite],
    ) {
        let pg_initial_writes: Vec<_> = connection
            .storage_logs_dedup_dal()
            .initial_writes_for_batch(l1_batch_number)
            .await;

        let pg_initial_writes: Option<Vec<_>> = pg_initial_writes
            .into_iter()
            .map(|(key, index)| {
                let key = U256::from_little_endian(key.as_bytes());
                Some((key, index?))
            })
            .collect();
        let Some(pg_initial_writes) = pg_initial_writes else {
            vlog::info!("Skipping indices consistency check as they are missing in Postgres for L1 batch {l1_batch_number}");
            return;
        };

        let tree_initial_writes: Vec<_> = tree_initial_writes
            .iter()
            .map(|write| (write.key, write.index))
            .collect();
        assert_eq!(
            pg_initial_writes, tree_initial_writes,
            "Leaf indices are not consistent for L1 batch {l1_batch_number}"
        );
    }
}
