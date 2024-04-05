//! Tree updater trait and its implementations.

use std::{ops, sync::Arc, time::Instant};

use anyhow::Context as _;
use futures::{future, FutureExt};
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_health_check::HealthUpdater;
use zksync_merkle_tree::domain::TreeMetadata;
use zksync_object_store::ObjectStore;
use zksync_types::{
    block::{L1BatchHeader, L1BatchTreeData},
    writes::InitialStorageWrite,
    L1BatchNumber, U256,
};

use super::{
    helpers::{AsyncTree, Delayer, L1BatchWithLogs},
    metrics::{TreeUpdateStage, METRICS},
    MetadataCalculator,
};
use crate::utils::wait_for_l1_batch;

#[derive(Debug)]
pub(super) struct TreeUpdater {
    tree: AsyncTree,
    max_l1_batches_per_iter: usize,
    object_store: Option<Arc<dyn ObjectStore>>,
}

impl TreeUpdater {
    pub fn new(
        tree: AsyncTree,
        max_l1_batches_per_iter: usize,
        object_store: Option<Arc<dyn ObjectStore>>,
    ) -> Self {
        Self {
            tree,
            max_l1_batches_per_iter,
            object_store,
        }
    }

    async fn process_l1_batch(
        &mut self,
        l1_batch: L1BatchWithLogs,
    ) -> anyhow::Result<(L1BatchHeader, TreeMetadata, Option<String>)> {
        let compute_latency = METRICS.start_stage(TreeUpdateStage::Compute);
        let l1_batch_header = l1_batch.header.clone();
        let l1_batch_number = l1_batch_header.number;
        let mut metadata = self.tree.process_l1_batch(l1_batch).await?;
        compute_latency.observe();

        let witness_input = metadata.witness.take();
        let object_key = if let Some(object_store) = &self.object_store {
            let witness_input =
                witness_input.context("no witness input provided by tree; this is a bug")?;
            let save_witnesses_latency = METRICS.start_stage(TreeUpdateStage::SaveGcs);
            let object_key = object_store
                .put(l1_batch_number, &witness_input)
                .await
                .context("cannot save witness input to object store")?;
            save_witnesses_latency.observe();

            tracing::info!(
                "Saved witnesses for L1 batch #{l1_batch_number} to object storage at `{object_key}`"
            );
            Some(object_key)
        } else {
            None
        };

        Ok((l1_batch_header, metadata, object_key))
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
        storage: &mut Connection<'_, Core>,
        l1_batch_numbers: ops::RangeInclusive<u32>,
    ) -> anyhow::Result<L1BatchNumber> {
        let tree_mode = self.tree.mode();
        let start = Instant::now();
        tracing::info!("Processing L1 batches #{l1_batch_numbers:?} in {tree_mode:?} mode");
        let first_l1_batch_number = L1BatchNumber(*l1_batch_numbers.start());
        let last_l1_batch_number = L1BatchNumber(*l1_batch_numbers.end());
        let mut l1_batch_data = L1BatchWithLogs::new(storage, first_l1_batch_number, tree_mode)
            .await
            .with_context(|| {
                format!("failed fetching tree input for L1 batch #{first_l1_batch_number}")
            })?;

        let mut total_logs = 0;
        let mut updated_headers = vec![];
        for l1_batch_number in l1_batch_numbers {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            let Some(current_l1_batch_data) = l1_batch_data else {
                return Ok(l1_batch_number);
            };
            total_logs += current_l1_batch_data.storage_logs.len();

            let process_l1_batch_task = self.process_l1_batch(current_l1_batch_data);
            let load_next_l1_batch_task = async {
                if l1_batch_number < last_l1_batch_number {
                    let next_l1_batch_number = l1_batch_number + 1;
                    L1BatchWithLogs::new(storage, next_l1_batch_number, tree_mode)
                        .await
                        .with_context(|| {
                            format!(
                                "failed fetching tree input for L1 batch #{next_l1_batch_number}"
                            )
                        })
                } else {
                    Ok(None) // Don't need to load the next L1 batch after the last one we're processing.
                }
            };
            let ((header, metadata, object_key), next_l1_batch_data) =
                future::try_join(process_l1_batch_task, load_next_l1_batch_task).await?;

            let check_consistency_latency = METRICS.start_stage(TreeUpdateStage::CheckConsistency);
            Self::check_initial_writes_consistency(
                storage,
                header.number,
                &metadata.initial_writes,
            )
            .await?;
            check_consistency_latency.observe();

            let save_postgres_latency = METRICS.start_stage(TreeUpdateStage::SavePostgres);
            let tree_data = L1BatchTreeData {
                hash: metadata.root_hash,
                rollup_last_leaf_index: metadata.rollup_last_leaf_index,
            };
            storage
                .blocks_dal()
                .save_l1_batch_tree_data(l1_batch_number, &tree_data)
                .await
                .context("failed saving tree data")?;
            // ^ Note that `save_l1_batch_tree_data()` will not blindly overwrite changes if L1 batch
            // metadata already exists; instead, it'll check that the old and new metadata match.
            // That is, if we run multiple tree instances, we'll get metadata correspondence
            // right away without having to implement dedicated code.

            if let Some(object_key) = &object_key {
                storage
                    .basic_witness_input_producer_dal()
                    .create_basic_witness_input_producer_job(l1_batch_number)
                    .await
                    .context("failed to create basic_witness_input_producer job")?;
                storage
                    .proof_generation_dal()
                    .insert_proof_generation_details(l1_batch_number, object_key)
                    .await;
            }
            save_postgres_latency.observe();
            tracing::info!("Updated metadata for L1 batch #{l1_batch_number} in Postgres");

            updated_headers.push(header);
            l1_batch_data = next_l1_batch_data;
        }

        let save_rocksdb_latency = METRICS.start_stage(TreeUpdateStage::SaveRocksdb);
        self.tree.save().await?;
        save_rocksdb_latency.observe();
        MetadataCalculator::update_metrics(&updated_headers, total_logs, start);

        Ok(last_l1_batch_number + 1)
    }

    async fn step(
        &mut self,
        mut storage: Connection<'_, Core>,
        next_l1_batch_to_seal: &mut L1BatchNumber,
    ) -> anyhow::Result<()> {
        let Some(last_sealed_l1_batch) = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("failed loading sealed L1 batch number")?
        else {
            tracing::trace!("No L1 batches to seal: Postgres storage is empty");
            return Ok(());
        };
        let last_requested_l1_batch =
            next_l1_batch_to_seal.0 + self.max_l1_batches_per_iter as u32 - 1;
        let last_requested_l1_batch = last_requested_l1_batch.min(last_sealed_l1_batch.0);
        let l1_batch_numbers = next_l1_batch_to_seal.0..=last_requested_l1_batch;
        if l1_batch_numbers.is_empty() {
            tracing::trace!(
                "No L1 batches to seal: batch numbers range to be loaded {l1_batch_numbers:?} is empty"
            );
        } else {
            tracing::info!("Updating Merkle tree with L1 batches #{l1_batch_numbers:?}");
            *next_l1_batch_to_seal = self
                .process_multiple_batches(&mut storage, l1_batch_numbers)
                .await?;
        }
        Ok(())
    }

    /// The processing loop for this updater.
    pub async fn loop_updating_tree(
        mut self,
        delayer: Delayer,
        pool: &ConnectionPool<Core>,
        mut stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
    ) -> anyhow::Result<()> {
        let Some(earliest_l1_batch) =
            wait_for_l1_batch(pool, delayer.delay_interval(), &mut stop_receiver).await?
        else {
            return Ok(()); // Stop signal received
        };
        let mut storage = pool.connection_tagged("metadata_calculator").await?;

        // Ensure genesis creation
        let tree = &mut self.tree;
        if tree.is_empty() {
            anyhow::ensure!(
                earliest_l1_batch == L1BatchNumber(0),
                "Non-zero earliest L1 batch #{earliest_l1_batch} is not supported without previous tree recovery"
            );
            let batch = L1BatchWithLogs::new(&mut storage, earliest_l1_batch, tree.mode())
                .await
                .with_context(|| {
                    format!("failed fetching tree input for L1 batch #{earliest_l1_batch}")
                })?
                .context("Missing storage logs for the genesis L1 batch")?;
            tree.process_l1_batch(batch).await?;
            tree.save().await?;
        }
        let mut next_l1_batch_to_seal = tree.next_l1_batch_number();

        let current_db_batch = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        let last_l1_batch_with_metadata = storage
            .blocks_dal()
            .get_last_l1_batch_number_with_metadata()
            .await?;
        drop(storage);

        tracing::info!(
            "Initialized metadata calculator with {max_batches_per_iter} max L1 batches per iteration. \
             Next L1 batch for Merkle tree: {next_l1_batch_to_seal}, current Postgres L1 batch: {current_db_batch:?}, \
             last L1 batch with metadata: {last_l1_batch_with_metadata:?}",
            max_batches_per_iter = self.max_l1_batches_per_iter
        );
        let tree_info = tree.reader().info().await;
        health_updater.update(tree_info.into());

        // It may be the case that we don't have any L1 batches with metadata in Postgres, e.g. after
        // recovering from a snapshot. We cannot wait for such a batch to appear (*this* is the component
        // responsible for their appearance!), but fortunately most of the updater doesn't depend on it.
        if let Some(last_l1_batch_with_metadata) = last_l1_batch_with_metadata {
            let backup_lag =
                (last_l1_batch_with_metadata.0 + 1).saturating_sub(next_l1_batch_to_seal.0);
            METRICS.backup_lag.set(backup_lag.into());

            if next_l1_batch_to_seal > last_l1_batch_with_metadata + 1 {
                // Check stop signal before proceeding with a potentially time-consuming operation.
                if *stop_receiver.borrow_and_update() {
                    tracing::info!("Stop signal received, metadata_calculator is shutting down");
                    return Ok(());
                }

                tracing::warn!(
                    "Next L1 batch of the tree ({next_l1_batch_to_seal}) is greater than last L1 batch with metadata in Postgres \
                     ({last_l1_batch_with_metadata}); this may be a result of restoring Postgres from a snapshot. \
                     Truncating Merkle tree versions so that this mismatch is fixed..."
                );
                tree.revert_logs(last_l1_batch_with_metadata);
                tree.save().await?;
                next_l1_batch_to_seal = tree.next_l1_batch_number();
                tracing::info!("Truncated Merkle tree to L1 batch #{next_l1_batch_to_seal}");

                let tree_info = tree.reader().info().await;
                health_updater.update(tree_info.into());
            }
        }

        loop {
            if *stop_receiver.borrow_and_update() {
                tracing::info!("Stop signal received, metadata_calculator is shutting down");
                break;
            }
            let storage = pool.connection_tagged("metadata_calculator").await?;

            let snapshot = *next_l1_batch_to_seal;
            self.step(storage, &mut next_l1_batch_to_seal).await?;
            let delay = if snapshot == *next_l1_batch_to_seal {
                tracing::trace!(
                    "Metadata calculator (next L1 batch: #{next_l1_batch_to_seal}) \
                     didn't make any progress; delaying it using {delayer:?}"
                );
                delayer.wait(&self.tree).left_future()
            } else {
                let tree_info = self.tree.reader().info().await;
                health_updater.update(tree_info.into());

                tracing::trace!(
                    "Metadata calculator (next L1 batch: #{next_l1_batch_to_seal}) made progress from #{snapshot}"
                );
                future::ready(()).right_future()
            };

            // The delays we're operating with are reasonably small, but selecting between the delay
            // and the stop receiver still allows to be more responsive during shutdown.
            tokio::select! {
                _ = stop_receiver.changed() => {
                    tracing::info!("Stop signal received, metadata_calculator is shutting down");
                    break;
                }
                () = delay => { /* The delay has passed */ }
            }
        }
        drop(health_updater); // Explicitly mark where the updater should be dropped
        Ok(())
    }

    async fn check_initial_writes_consistency(
        connection: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        tree_initial_writes: &[InitialStorageWrite],
    ) -> anyhow::Result<()> {
        let pg_initial_writes = connection
            .storage_logs_dedup_dal()
            .initial_writes_for_batch(l1_batch_number)
            .await
            .context("cannot get initial writes for L1 batch")?;

        let pg_initial_writes: Vec<_> = pg_initial_writes
            .into_iter()
            .map(|(key, index)| {
                let key = U256::from_little_endian(key.as_bytes());
                (key, index)
            })
            .collect();

        let tree_initial_writes: Vec<_> = tree_initial_writes
            .iter()
            .map(|write| (write.key, write.index))
            .collect();
        anyhow::ensure!(
            pg_initial_writes == tree_initial_writes,
            "Leaf indices are not consistent for L1 batch {l1_batch_number}.\n\
             Postgres writes are: {pg_initial_writes:?}\n\
             Tree writes are: {tree_initial_writes:?}"
        );
        Ok(())
    }
}
