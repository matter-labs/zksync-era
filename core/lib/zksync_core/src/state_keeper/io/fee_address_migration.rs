//! Temporary module for migrating fee addresses from L1 batches to miniblocks.

use std::{
    future,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{Address, MiniblockNumber};

use crate::utils::{binary_search_with, BinarySearchPredicate};

/// Runs the migration for pending miniblocks.
pub(crate) async fn migrate_pending_miniblocks(storage: &mut StorageProcessor<'_>) {
    let started_at = Instant::now();
    tracing::info!("Started migrating `fee_account_address` for pending miniblocks");

    // FIXME: should work after 2nd DB migration (currently, will panic)
    let rows_affected = storage
        .blocks_dal()
        .copy_fee_account_address_for_pending_miniblocks()
        .await
        .expect("Failed migrating `fee_account_address` for pending miniblocks");
    let elapsed = started_at.elapsed();
    tracing::info!("Migrated `fee_account_address` for {rows_affected} miniblocks in {elapsed:?}");
}

/// Runs the migration for non-pending miniblocks. Must be run as a background task.
// FIXME: test
pub(crate) async fn migrate_miniblocks(
    pool: ConnectionPool,
    last_miniblock: MiniblockNumber,
    chunk_size: u32,
    sleep_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    anyhow::ensure!(chunk_size > 0, "Chunk size must be positive");

    let mut storage = pool.access_storage().await?;
    // Check whether the last miniblock is migrated. If it is, we can finish early.
    // This will also protect us from running unsupported `BlocksDal` queries after the 2nd DB migration is run
    // removing `l1_batches.fee_account_address`.
    if is_fee_address_migrated(&mut storage, last_miniblock).await? {
        tracing::info!(
            "All miniblocks in 0..={last_miniblock} have `fee_account_address` migrated"
        );
        return Ok(());
    }

    // At this point, we have at least one non-migrated miniblock. Find the non-migrated range
    // using binary search.
    let last_migrated_miniblock =
        binary_search_with(0, last_miniblock.0, MigratedMiniblockSearch(storage)).await?;

    let mut first_miniblock_to_migrate = MiniblockNumber(last_migrated_miniblock) + 1;
    tracing::info!(
        "Migrating `fee_account_address` for miniblocks {first_miniblock_to_migrate}..={last_miniblock} \
         in chunks of {chunk_size} miniblocks"
    );
    while first_miniblock_to_migrate <= last_miniblock {
        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received; migration shutting down");
            return Ok(());
        }

        let chunk = first_miniblock_to_migrate
            ..=last_miniblock.min(first_miniblock_to_migrate + chunk_size - 1);
        tracing::debug!("Migrating `fee_account_address` for miniblocks chunk {chunk:?}");

        let rows_affected = pool
            .access_storage()
            .await?
            .blocks_dal()
            .copy_fee_account_address_for_miniblocks(chunk.clone())
            .await
            .with_context(|| format!("Failed migrating miniblocks chunk {chunk:?}"))?;
        tracing::debug!("Migrated {rows_affected} miniblocks in chunk {chunk:?}");

        first_miniblock_to_migrate = *chunk.end() + 1;
        tokio::time::sleep(sleep_interval).await;
    }

    tracing::info!("Migrated `fee_account_address` for all miniblocks");
    future::pending::<()>().await;
    // ^ Since we run as a task, we don't want to exit early (this will stop the node).
    Ok(())
}

async fn is_fee_address_migrated(
    storage: &mut StorageProcessor<'_>,
    miniblock: MiniblockNumber,
) -> anyhow::Result<bool> {
    let fee_address = storage
        .blocks_dal()
        .get_fee_address_for_miniblock(miniblock)
        .await
        .with_context(|| format!("Failed getting fee address for miniblock #{miniblock}"))?
        .with_context(|| format!("Miniblock #{miniblock} disappeared"))?;
    Ok(fee_address != Address::default())
}

#[derive(Debug)]
struct MigratedMiniblockSearch<'a>(StorageProcessor<'a>);

#[async_trait]
impl BinarySearchPredicate for MigratedMiniblockSearch<'_> {
    type Error = anyhow::Error;

    async fn eval(&mut self, argument: u32) -> Result<bool, Self::Error> {
        is_fee_address_migrated(&mut self.0, MiniblockNumber(argument)).await
    }
}
