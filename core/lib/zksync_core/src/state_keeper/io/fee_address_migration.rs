//! Temporary module for migrating fee addresses from L1 batches to miniblocks.

use std::{
    ops,
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

/// Runs the migration for non-pending miniblocks. Should be run as a background task.
pub(crate) async fn migrate_miniblocks(
    pool: ConnectionPool,
    last_miniblock: MiniblockNumber,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let MigrationOutput {
        migrated_range,
        miniblocks_affected,
    } = migrate_miniblocks_inner(
        pool,
        last_miniblock,
        100_000,
        Duration::from_secs(1),
        stop_receiver,
    )
    .await?;

    tracing::info!(
        "Finished fee address migration for range {migrated_range:?} with {miniblocks_affected} affected miniblocks"
    );
    Ok(())
}

#[derive(Debug, Default)]
struct MigrationOutput {
    migrated_range: Option<ops::RangeInclusive<MiniblockNumber>>,
    miniblocks_affected: u64,
}

async fn migrate_miniblocks_inner(
    pool: ConnectionPool,
    last_miniblock: MiniblockNumber,
    chunk_size: u32,
    sleep_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<MigrationOutput> {
    anyhow::ensure!(chunk_size > 0, "Chunk size must be positive");

    let mut storage = pool.access_storage().await?;
    // Check whether the last miniblock is migrated. If it is, we can finish early.
    // This will also protect us from running unsupported `BlocksDal` queries after the 2nd DB migration is run
    // removing `l1_batches.fee_account_address`.
    if is_fee_address_migrated(&mut storage, last_miniblock).await? {
        tracing::info!(
            "All miniblocks in 0..={last_miniblock} have `fee_account_address` migrated"
        );
        return Ok(MigrationOutput::default());
    }

    // At this point, we have at least one non-migrated miniblock. Find the non-migrated range
    // using binary search.
    let last_migrated_miniblock =
        binary_search_with(0, last_miniblock.0, MigratedMiniblockSearch(storage)).await?;

    let first_miniblock_to_migrate = if last_migrated_miniblock > 0 {
        MiniblockNumber(last_migrated_miniblock) + 1
    } else {
        MiniblockNumber(0) // If binary search yielded 0, we may have no migrated miniblocks
    };
    let mut chunk_start = first_miniblock_to_migrate;
    let mut miniblocks_affected = 0;

    tracing::info!(
        "Migrating `fee_account_address` for miniblocks {first_miniblock_to_migrate}..={last_miniblock} \
         in chunks of {chunk_size} miniblocks"
    );
    while chunk_start <= last_miniblock {
        let chunk_end = last_miniblock.min(chunk_start + chunk_size - 1);
        let chunk = chunk_start..=chunk_end;
        tracing::debug!("Migrating `fee_account_address` for miniblocks chunk {chunk:?}");

        let rows_affected = pool
            .access_storage()
            .await?
            .blocks_dal()
            .copy_fee_account_address_for_miniblocks(chunk.clone())
            .await
            .with_context(|| format!("Failed migrating miniblocks chunk {chunk:?}"))?;
        tracing::debug!("Migrated {rows_affected} miniblocks in chunk {chunk:?}");
        miniblocks_affected += rows_affected;

        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received; fee address migration shutting down");
            return Ok(MigrationOutput {
                migrated_range: Some(first_miniblock_to_migrate..=chunk_end),
                miniblocks_affected,
            });
        }
        chunk_start = *chunk.end() + 1;
        tokio::time::sleep(sleep_interval).await;
    }

    Ok(MigrationOutput {
        migrated_range: Some(first_miniblock_to_migrate..=last_miniblock),
        miniblocks_affected,
    })
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

#[cfg(test)]
mod tests {
    use test_casing::test_casing;
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::{BlockGasCount, L1BatchHeader},
        L1BatchNumber, ProtocolVersion, ProtocolVersionId,
    };

    use super::*;
    use crate::utils::testonly::create_miniblock;

    async fn prepare_storage(storage: &mut StorageProcessor<'_>) {
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        for number in 0..5 {
            let miniblock = create_miniblock(number);
            storage
                .blocks_dal()
                .insert_miniblock(&miniblock)
                .await
                .unwrap();

            let l1_batch = L1BatchHeader::new(
                L1BatchNumber(number),
                number.into(),
                BaseSystemContractsHashes::default(),
                ProtocolVersionId::latest(),
            );
            storage
                .blocks_dal()
                .insert_l1_batch(&l1_batch, &[], BlockGasCount::default(), &[], &[])
                .await
                .unwrap();
            storage
                .blocks_dal()
                .set_l1_batch_fee_address(
                    l1_batch.number,
                    Address::from_low_u64_be(u64::from(number) + 1),
                )
                .await
                .unwrap();
            storage
                .blocks_dal()
                .mark_miniblocks_as_executed_in_l1_batch(l1_batch.number)
                .await
                .unwrap();
        }
    }

    async fn assert_migration(storage: &mut StorageProcessor<'_>) {
        for number in 0..5 {
            let fee_address = storage
                .blocks_dal()
                .get_fee_address_for_miniblock(MiniblockNumber(number))
                .await
                .unwrap()
                .expect("no fee address");
            let expected_address = Address::from_low_u64_be(u64::from(number) + 1);
            assert_eq!(fee_address, expected_address);
        }
    }

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn migration_basics(chunk_size: u32) {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        prepare_storage(&mut storage).await;
        drop(storage);

        let (_stop_sender, stop_receiver) = watch::channel(false);
        let result = migrate_miniblocks_inner(
            pool.clone(),
            MiniblockNumber(4),
            chunk_size,
            Duration::ZERO,
            stop_receiver.clone(),
        )
        .await
        .unwrap();

        assert_eq!(result.miniblocks_affected, 5);

        // Check that all blocks are migrated.
        let mut storage = pool.access_storage().await.unwrap();
        assert_migration(&mut storage).await;
        drop(storage);

        // Check that migration can run again w/o returning an error, hanging up etc.
        let result = migrate_miniblocks_inner(
            pool.clone(),
            MiniblockNumber(4),
            chunk_size,
            Duration::ZERO,
            stop_receiver,
        )
        .await
        .unwrap();

        assert_eq!(result.miniblocks_affected, 0);
    }

    #[tokio::test]
    async fn stopping_and_resuming_migration() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        prepare_storage(&mut storage).await;
        drop(storage);

        let chunk_size = 2;
        let (_stop_sender, stop_receiver) = watch::channel(true); // signal stop right away
        let result = migrate_miniblocks_inner(
            pool.clone(),
            MiniblockNumber(4),
            chunk_size,
            Duration::from_secs(1_000),
            stop_receiver,
        )
        .await
        .unwrap();

        // Migration should stop after a single chunk.
        assert_eq!(result.miniblocks_affected, u64::from(chunk_size));
        assert_eq!(
            result.migrated_range.unwrap(),
            MiniblockNumber(0)..=MiniblockNumber(chunk_size - 1)
        );

        // Check that migration resumes from the same point.
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let result = migrate_miniblocks_inner(
            pool.clone(),
            MiniblockNumber(4),
            chunk_size,
            Duration::ZERO,
            stop_receiver,
        )
        .await
        .unwrap();

        assert_eq!(result.miniblocks_affected, 3);
        assert_eq!(
            result.migrated_range.unwrap(),
            MiniblockNumber(chunk_size)..=MiniblockNumber(4)
        );
    }
}
