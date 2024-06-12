//! Tree updater trait and its implementations.

use std::{ops, sync::Arc, time::Instant};

use anyhow::Context as _;
use futures::{future, FutureExt};
use tokio::sync::watch;
use zksync_dal::{helpers::wait_for_l1_batch, Connection, ConnectionPool, Core, CoreDal};
use zksync_merkle_tree::domain::TreeMetadata;
use zksync_object_store::ObjectStore;
use zksync_types::{
    block::{L1BatchHeader, L1BatchTreeData},
    L1BatchNumber,
};

use super::{
    helpers::{AsyncTree, Delayer, L1BatchWithLogs},
    metrics::{TreeUpdateStage, METRICS},
    MetadataCalculator,
};

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
                    .tee_verifier_input_producer_dal()
                    .create_tee_verifier_input_producer_job(l1_batch_number)
                    .await
                    .expect("failed to create tee_verifier_input_producer job");
                // Save the proof generation details to Postgres
                storage
                    .proof_generation_dal()
                    .insert_proof_generation_details(l1_batch_number, object_key)
                    .await?;
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
    ) -> anyhow::Result<()> {
        let tree = &mut self.tree;
        let mut next_l1_batch_to_seal = tree.next_l1_batch_number();
        tracing::info!(
            "Initialized metadata calculator with {max_batches_per_iter} max L1 batches per iteration. \
             Next L1 batch for Merkle tree: {next_l1_batch_to_seal}",
            max_batches_per_iter = self.max_l1_batches_per_iter
        );

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
        Ok(())
    }
}

impl AsyncTree {
    async fn ensure_genesis(
        &mut self,
        storage: &mut Connection<'_, Core>,
        earliest_l1_batch: L1BatchNumber,
    ) -> anyhow::Result<()> {
        if !self.is_empty() {
            return Ok(());
        }

        anyhow::ensure!(
            earliest_l1_batch == L1BatchNumber(0),
            "Non-zero earliest L1 batch #{earliest_l1_batch} is not supported without previous tree recovery"
        );
        let batch = L1BatchWithLogs::new(storage, earliest_l1_batch, self.mode())
            .await
            .with_context(|| {
                format!("failed fetching tree input for L1 batch #{earliest_l1_batch}")
            })?
            .context("Missing storage logs for the genesis L1 batch")?;
        self.process_l1_batch(batch).await?;
        self.save().await?;
        Ok(())
    }

    /// Invariant: the tree is not ahead of Postgres.
    async fn ensure_no_l1_batch_divergence(
        &mut self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<()> {
        let Some(last_tree_l1_batch) = self.next_l1_batch_number().checked_sub(1) else {
            // No L1 batches in the tree means no divergence.
            return Ok(());
        };
        let last_tree_l1_batch = L1BatchNumber(last_tree_l1_batch);

        let mut storage = pool.connection_tagged("metadata_calculator").await?;
        if self
            .l1_batch_matches(&mut storage, last_tree_l1_batch)
            .await?
        {
            tracing::debug!(
                "Last l1 batch in tree #{last_tree_l1_batch} has same data in tree and Postgres"
            );
            return Ok(());
        }

        tracing::debug!("Last l1 batch in tree #{last_tree_l1_batch} has diverging data in tree and Postgres; searching for the last common L1 batch");
        let min_tree_l1_batch = self
            .min_l1_batch_number()
            .context("tree shouldn't be empty at this point")?;
        anyhow::ensure!(
            min_tree_l1_batch <= last_tree_l1_batch,
            "potential Merkle tree corruption: minimum L1 batch number ({min_tree_l1_batch}) exceeds the last L1 batch ({last_tree_l1_batch})"
        );

        anyhow::ensure!(
            self.l1_batch_matches(&mut storage, min_tree_l1_batch).await?,
            "diverging min L1 batch in the tree #{min_tree_l1_batch}; the tree cannot recover from this"
        );

        let mut left = min_tree_l1_batch.0;
        let mut right = last_tree_l1_batch.0;
        while left + 1 < right {
            let middle = (left + right) / 2;
            let batch_matches = self
                .l1_batch_matches(&mut storage, L1BatchNumber(middle))
                .await?;
            if batch_matches {
                left = middle;
            } else {
                right = middle;
            }
        }
        let last_common_l1_batch_number = L1BatchNumber(left);
        tracing::info!("Found last common L1 batch between tree and Postgres: #{last_common_l1_batch_number}; will revert tree to it");

        self.roll_back_logs(last_common_l1_batch_number)?;
        self.save().await?;
        Ok(())
    }

    async fn l1_batch_matches(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch: L1BatchNumber,
    ) -> anyhow::Result<bool> {
        if l1_batch == L1BatchNumber(0) {
            // Corner case: root hash for L1 batch #0 persisted in Postgres is fictive (set to `H256::zero()`).
            return Ok(true);
        }

        let Some(tree_data) = self.data_for_l1_batch(l1_batch) else {
            // Corner case: the L1 batch was pruned in the tree.
            return Ok(true);
        };
        let Some(tree_data_from_postgres) = storage
            .blocks_dal()
            .get_l1_batch_tree_data(l1_batch)
            .await?
        else {
            // Corner case: the L1 batch was pruned in Postgres (including initial snapshot recovery).
            return Ok(true);
        };

        let data_matches = tree_data == tree_data_from_postgres;
        if !data_matches {
            tracing::warn!(
                "Detected diverging tree data for L1 batch #{l1_batch}; data in tree is: {tree_data:?}, \
                 data in Postgres is: {tree_data_from_postgres:?}"
            );
        }
        Ok(data_matches)
    }

    /// Ensures that the tree is consistent with Postgres, truncating the tree if necessary.
    /// This will wait for at least one L1 batch to appear in Postgres if necessary.
    pub(crate) async fn ensure_consistency(
        &mut self,
        delayer: &Delayer,
        pool: &ConnectionPool<Core>,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let Some(earliest_l1_batch) =
            wait_for_l1_batch(pool, delayer.delay_interval(), stop_receiver).await?
        else {
            return Ok(()); // Stop signal received
        };
        let mut storage = pool.connection_tagged("metadata_calculator").await?;

        self.ensure_genesis(&mut storage, earliest_l1_batch).await?;
        let next_l1_batch_to_seal = self.next_l1_batch_number();

        let current_db_batch = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        let last_l1_batch_with_tree_data = storage
            .blocks_dal()
            .get_last_l1_batch_number_with_tree_data()
            .await?;
        drop(storage);

        tracing::info!(
            "Next L1 batch for Merkle tree: {next_l1_batch_to_seal}, current Postgres L1 batch: {current_db_batch:?}, \
             last L1 batch with metadata: {last_l1_batch_with_tree_data:?}"
        );

        // It may be the case that we don't have any L1 batches with metadata in Postgres, e.g. after
        // recovering from a snapshot. We cannot wait for such a batch to appear (*this* is the component
        // responsible for their appearance!), but fortunately most of the updater doesn't depend on it.
        if let Some(last_l1_batch_with_tree_data) = last_l1_batch_with_tree_data {
            let backup_lag =
                (last_l1_batch_with_tree_data.0 + 1).saturating_sub(next_l1_batch_to_seal.0);
            METRICS.backup_lag.set(backup_lag.into());

            if next_l1_batch_to_seal > last_l1_batch_with_tree_data + 1 {
                tracing::warn!(
                    "Next L1 batch of the tree ({next_l1_batch_to_seal}) is greater than last L1 batch with metadata in Postgres \
                     ({last_l1_batch_with_tree_data}); this may be a result of restoring Postgres from a snapshot. \
                     Truncating Merkle tree versions so that this mismatch is fixed..."
                );
                self.roll_back_logs(last_l1_batch_with_tree_data)?;
                self.save().await?;
                tracing::info!("Truncated Merkle tree to L1 batch #{next_l1_batch_to_seal}");
            }

            self.ensure_no_l1_batch_divergence(pool).await?;
        }
        Ok(())
    }
}
