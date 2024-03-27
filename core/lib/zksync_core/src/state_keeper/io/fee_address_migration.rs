//! Temporary module for migrating fee addresses from L1 batches to miniblocks.

// FIXME (PLA-728): remove after 2nd phase of `fee_account_address` migration

use std::time::{Duration, Instant};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::MiniblockNumber;

/// Runs the migration for pending miniblocks.
pub(crate) async fn migrate_pending_miniblocks(
    storage: &mut Connection<'_, Core>,
) -> anyhow::Result<()> {
    let started_at = Instant::now();
    tracing::info!("Started migrating `fee_account_address` for pending miniblocks");

    #[allow(deprecated)]
    let l1_batches_have_fee_account_address = storage
        .blocks_dal()
        .check_l1_batches_have_fee_account_address()
        .await
        .context("failed getting metadata for l1_batches table")?;
    if !l1_batches_have_fee_account_address {
        tracing::info!("`l1_batches.fee_account_address` column is removed; assuming that the migration is complete");
        return Ok(());
    }

    #[allow(deprecated)]
    let rows_affected = storage
        .blocks_dal()
        .copy_fee_account_address_for_pending_miniblocks()
        .await
        .context("failed migrating `fee_account_address` for pending miniblocks")?;
    let elapsed = started_at.elapsed();
    tracing::info!("Migrated `fee_account_address` for {rows_affected} miniblocks in {elapsed:?}");
    Ok(())
}

/// Runs the migration for non-pending miniblocks. Should be run as a background task.
pub(crate) async fn migrate_miniblocks(
    pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    // `migrate_miniblocks_inner` assumes that miniblocks start from the genesis (i.e., no snapshot recovery).
    // Since snapshot recovery is later that the fee address migration in terms of code versioning,
    // the migration is always no-op in case of snapshot recovery; all miniblocks added after recovery are guaranteed
    // to have their fee address set.
    let mut storage = pool.connection_tagged("state_keeper").await?;
    if storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await?
        .is_some()
    {
        tracing::info!("Detected snapshot recovery; fee address migration is skipped as no-op");
        return Ok(());
    }
    let last_miniblock = storage
        .blocks_dal()
        .get_sealed_miniblock_number()
        .await?
        .context("storage is empty, but there's no snapshot recovery data")?;
    drop(storage);

    let MigrationOutput {
        miniblocks_affected,
    } = migrate_miniblocks_inner(
        pool,
        last_miniblock,
        100_000,
        Duration::from_secs(1),
        stop_receiver,
    )
    .await?;

    tracing::info!("Finished fee address migration with {miniblocks_affected} affected miniblocks");
    Ok(())
}

#[derive(Debug, Default)]
struct MigrationOutput {
    miniblocks_affected: u64,
}

/// It's important for the `chunk_size` to be a constant; this ensures that each chunk is migrated atomically.
async fn migrate_miniblocks_inner(
    pool: ConnectionPool<Core>,
    last_miniblock: MiniblockNumber,
    chunk_size: u32,
    sleep_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<MigrationOutput> {
    anyhow::ensure!(chunk_size > 0, "Chunk size must be positive");

    let mut storage = pool.connection_tagged("state_keeper").await?;
    #[allow(deprecated)]
    let l1_batches_have_fee_account_address = storage
        .blocks_dal()
        .check_l1_batches_have_fee_account_address()
        .await
        .context("Failed getting metadata for l1_batches table")?;
    drop(storage);
    if !l1_batches_have_fee_account_address {
        tracing::info!("`l1_batches.fee_account_address` column is removed; assuming that the migration is complete");
        return Ok(MigrationOutput::default());
    }

    let mut chunk_start = MiniblockNumber(0);
    let mut miniblocks_affected = 0;

    tracing::info!(
        "Migrating `fee_account_address` for miniblocks {chunk_start}..={last_miniblock} \
         in chunks of {chunk_size} miniblocks"
    );
    while chunk_start <= last_miniblock {
        let chunk_end = last_miniblock.min(chunk_start + chunk_size - 1);
        let chunk = chunk_start..=chunk_end;

        let mut storage = pool.connection_tagged("state_keeper").await?;
        let is_chunk_migrated = is_fee_address_migrated(&mut storage, chunk_start).await?;

        if is_chunk_migrated {
            tracing::debug!("`fee_account_address` is migrated for chunk {chunk:?}");
        } else {
            tracing::debug!("Migrating `fee_account_address` for miniblocks chunk {chunk:?}");

            #[allow(deprecated)]
            let rows_affected = storage
                .blocks_dal()
                .copy_fee_account_address_for_miniblocks(chunk.clone())
                .await
                .with_context(|| format!("Failed migrating miniblocks chunk {chunk:?}"))?;
            tracing::debug!("Migrated {rows_affected} miniblocks in chunk {chunk:?}");
            miniblocks_affected += rows_affected;
        }
        drop(storage);

        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received; fee address migration shutting down");
            return Ok(MigrationOutput {
                miniblocks_affected,
            });
        }
        chunk_start = chunk_end + 1;

        if !is_chunk_migrated {
            tokio::time::sleep(sleep_interval).await;
        }
    }

    Ok(MigrationOutput {
        miniblocks_affected,
    })
}

#[allow(deprecated)]
async fn is_fee_address_migrated(
    storage: &mut Connection<'_, Core>,
    miniblock: MiniblockNumber,
) -> anyhow::Result<bool> {
    storage
        .blocks_dal()
        .is_fee_address_migrated(miniblock)
        .await
        .with_context(|| format!("Failed getting fee address for miniblock #{miniblock}"))?
        .with_context(|| format!("Miniblock #{miniblock} disappeared"))
}

#[cfg(test)]
mod tests {
    use test_casing::test_casing;
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::L1BatchHeader, Address, L1BatchNumber, ProtocolVersion, ProtocolVersionId,
    };

    use super::*;
    use crate::utils::testonly::create_miniblock;

    async fn prepare_storage(storage: &mut Connection<'_, Core>) {
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
                .insert_mock_l1_batch(&l1_batch)
                .await
                .unwrap();
            #[allow(deprecated)]
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

    async fn assert_migration(storage: &mut Connection<'_, Core>) {
        for number in 0..5 {
            assert!(is_fee_address_migrated(storage, MiniblockNumber(number))
                .await
                .unwrap());

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
        // Replicate providing a pool with a single connection.
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
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
        let mut storage = pool.connection().await.unwrap();
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

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn stopping_and_resuming_migration(chunk_size: u32) {
        let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        prepare_storage(&mut storage).await;
        drop(storage);

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

        assert_eq!(result.miniblocks_affected, 5 - u64::from(chunk_size));
        let mut storage = pool.connection().await.unwrap();
        assert_migration(&mut storage).await;
    }

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn new_blocks_added_during_migration(chunk_size: u32) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        prepare_storage(&mut storage).await;

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

        // Insert a new miniblock to the storage with a defined fee account address.
        let mut miniblock = create_miniblock(5);
        miniblock.fee_account_address = Address::repeat_byte(1);
        storage
            .blocks_dal()
            .insert_miniblock(&miniblock)
            .await
            .unwrap();

        // Resume the migration.
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let result = migrate_miniblocks_inner(
            pool.clone(),
            MiniblockNumber(5),
            chunk_size,
            Duration::ZERO,
            stop_receiver,
        )
        .await
        .unwrap();

        // The new miniblock should not be affected.
        assert_eq!(result.miniblocks_affected, 5 - u64::from(chunk_size));
        assert_migration(&mut storage).await;
    }
}
