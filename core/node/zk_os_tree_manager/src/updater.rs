use std::{future, time::Duration};

use anyhow::Context;
use futures::FutureExt;
use tokio::sync::watch;
use zksync_dal::{helpers::wait_for_l1_batch, Connection, ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use crate::helpers::AsyncMerkleTree;

#[derive(Debug)]
pub(crate) struct TreeUpdater {
    tree: AsyncMerkleTree,
    max_l1_batches_per_iter: usize,
}

impl TreeUpdater {
    pub(crate) fn new(tree: AsyncMerkleTree, max_l1_batches_per_iter: usize) -> Self {
        Self {
            tree,
            max_l1_batches_per_iter,
        }
    }

    pub(crate) async fn loop_updating_tree(
        mut self,
        delay_interval: Duration,
        pool: &ConnectionPool<Core>,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let tree = &mut self.tree;
        let mut next_l1_batch_to_process = tree.next_l1_batch_number().await?;
        tracing::info!(
            "Initialized metadata calculator with {max_l1_batches_per_iter} max L1 batches per iteration. \
             Next L1 batch for Merkle tree: {next_l1_batch_to_process}",
            max_l1_batches_per_iter = self.max_l1_batches_per_iter
        );

        loop {
            if *stop_receiver.borrow_and_update() {
                tracing::info!("Stop signal received, metadata_calculator is shutting down");
                break;
            }

            let snapshot = *next_l1_batch_to_process;
            self.step(pool, &mut next_l1_batch_to_process).await?;
            let delay = if snapshot == *next_l1_batch_to_process {
                tracing::trace!(
                    "Tree updater (next L1 batch: #{next_l1_batch_to_process}) \
                     didn't make any progress; delaying it using {delay_interval:?}"
                );
                tokio::time::sleep(delay_interval).left_future()
            } else {
                tracing::trace!(
                    "Metadata calculator (next L1 batch: #{next_l1_batch_to_process}) made progress from #{snapshot}"
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

impl AsyncMerkleTree {
    async fn ensure_genesis(
        &mut self,
        storage: &mut Connection<'_, Core>,
        earliest_l1_batch: L1BatchNumber,
    ) -> anyhow::Result<()> {
        if !self.is_empty().await? {
            return Ok(());
        }

        anyhow::ensure!(
            earliest_l1_batch == L1BatchNumber(0),
            "Non-zero earliest L1 batch #{earliest_l1_batch} is not supported, and tree recovery isn't supported either"
        );
        let batch = L1BatchWithLogs::new(storage, earliest_l1_batch)
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
        let Some(last_tree_l1_batch) = self.next_l1_batch_number().await?.checked_sub(1) else {
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
            .await?
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

        self.roll_back_logs(last_common_l1_batch_number).await?;
        self.save().await?;
        Ok(())
    }

    async fn l1_batch_matches(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_batch: L1BatchNumber,
    ) -> anyhow::Result<bool> {
        if l1_batch == L1BatchNumber(0) {
            // Corner case: root hash for L1 batch #0 persisted in Postgres is fictive (set to `H256::zero()`).
            return Ok(true);
        }

        let Some(tree_data) = self.data_for_l1_batch(l1_batch).await? else {
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
        delay_interval: Duration,
        pool: &ConnectionPool<Core>,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let Some(earliest_l1_batch) =
            wait_for_l1_batch(pool, delay_interval, stop_receiver).await?
        else {
            return Ok(()); // Stop signal received
        };
        let mut storage = pool.connection_tagged("tree_updater").await?;

        self.ensure_genesis(&mut storage, earliest_l1_batch).await?;
        let next_l1_batch_to_process = self.next_l1_batch_number().await?;

        let current_db_batch = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
            .unwrap_or_default();
        let last_l1_batch_with_tree_data = storage
            .blocks_dal()
            .get_last_l1_batch_number_with_tree_data()
            .await?;
        drop(storage);

        tracing::info!(
            "Next L1 batch for Merkle tree: {next_l1_batch_to_process}, current Postgres L1 batch: {current_db_batch:?}, \
             last L1 batch with metadata: {last_l1_batch_with_tree_data:?}"
        );

        // It may be the case that we don't have any L1 batches with metadata in Postgres, e.g. after
        // recovering from a snapshot. We cannot wait for such a batch to appear (*this* is the component
        // responsible for their appearance!), but fortunately most of the updater doesn't depend on it.
        if let Some(last_l1_batch_with_tree_data) = last_l1_batch_with_tree_data {
            // FIXME: uncomment
            //let backup_lag =
            //    (last_l1_batch_with_tree_data.0 + 1).saturating_sub(next_l1_batch_to_process.0);
            //METRICS.backup_lag.set(backup_lag.into());

            if next_l1_batch_to_process > last_l1_batch_with_tree_data + 1 {
                tracing::warn!(
                    "Next L1 batch of the tree ({next_l1_batch_to_process}) is greater than last L1 batch with metadata in Postgres \
                     ({last_l1_batch_with_tree_data}); this may be a result of restoring Postgres from a snapshot. \
                     Truncating Merkle tree versions so that this mismatch is fixed..."
                );
                self.roll_back_logs(last_l1_batch_with_tree_data).await?;
                self.save().await?;
                tracing::info!("Truncated Merkle tree to L1 batch #{next_l1_batch_to_process}");
            }

            self.ensure_no_l1_batch_divergence(pool).await?;
        }
        Ok(())
    }
}
