//! Temporary module for migrating fee addresses from L1 batches to miniblocks.

use std::time::Duration;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::MiniblockNumber;

/// Runs the migration for non-pending miniblocks. Should be run as a background task.
pub(crate) async fn migrate_miniblocks(
    pool: ConnectionPool,
    last_miniblock: MiniblockNumber,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let MigrationOutput { events_affected } = migrate_miniblocks_inner(
        pool,
        last_miniblock,
        100_000,
        Duration::from_secs(1),
        stop_receiver,
    )
    .await?;

    tracing::info!("Finished event indexes migration with {events_affected} affected events");
    Ok(())
}

#[derive(Debug, Default)]
struct MigrationOutput {
    events_affected: u64,
}

/// It's important for the `chunk_size` to be a constant; this ensures that each chunk is migrated atomically.
async fn migrate_miniblocks_inner(
    pool: ConnectionPool,
    last_miniblock: MiniblockNumber,
    chunk_size: u32,
    sleep_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<MigrationOutput> {
    anyhow::ensure!(chunk_size > 0, "Chunk size must be positive");

    let mut chunk_start = MiniblockNumber(0);
    let mut miniblocks_affected = 0;

    tracing::info!(
        "Reassigning log indexes without ETH transfer for miniblocks {chunk_start}..={last_miniblock} \
         in chunks of {chunk_size} miniblocks"
    );

    while chunk_start <= last_miniblock {
        let chunk_end = last_miniblock.min(chunk_start + chunk_size - 1);
        let chunk = chunk_start..=chunk_end;

        let mut storage = pool.access_storage().await?;
        let is_chunk_migrated = are_event_indexes_migrated(&mut storage, chunk_start).await?;

        if is_chunk_migrated {
            tracing::debug!("Event indexes are migrated for chunk {chunk:?}");
        } else {
            tracing::debug!("Migrating event indexes for miniblocks chunk {chunk:?}");

            #[allow(deprecated)]
            let rows_affected = storage
                .events_dal()
                .assign_indexes_without_eth_transfer(chunk.clone())
                .await
                .with_context(|| {
                    format!("Failed migrating events in miniblocks, chunk {chunk:?}")
                })?;
            tracing::debug!("Migrated {rows_affected} miniblocks in chunk {chunk:?}");
            miniblocks_affected += rows_affected;
        }
        drop(storage);

        if *stop_receiver.borrow() {
            tracing::info!("Stop signal received; event index migration shutting down");
            return Ok(MigrationOutput { events_affected });
        }
        chunk_start = chunk_end + 1;

        if !is_chunk_migrated {
            tokio::time::sleep(sleep_interval).await;
        }
    }

    Ok(MigrationOutput { events_affected })
}

#[allow(deprecated)]
async fn are_event_indexes_migrated(
    storage: &mut StorageProcessor<'_>,
    miniblock: MiniblockNumber,
) -> anyhow::Result<bool> {
    storage
        .events_dal()
        .are_event_indexes_migrated(miniblock)
        .await
        .with_context(|| format!("Failed getting event indexes for miniblock #{miniblock}"))
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use multivm::zk_evm_1_3_1::ethereum_types::H256;
    use test_casing::test_casing;
    use zksync_dal::transactions_dal::L2TxSubmissionResult;
    use zksync_system_constants::{L2_ETH_TOKEN_ADDRESS, TRANSFER_EVENT_TOPIC};
    use zksync_types::{
        block::{BlockGasCount, MiniblockHeader},
        fee::TransactionExecutionMetrics,
        tx::IncludedTxLocation,
        Address, L1BatchNumber, ProtocolVersion, VmEvent,
    };

    use super::*;
    use crate::utils::testonly::{create_l1_batch, create_l2_transaction, create_miniblock};

    async fn store_miniblock(
        storage: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<(MiniblockHeader, H256)> {
        let new_tx = create_l2_transaction(1, 2);
        let new_tx_hash = new_tx.hash();
        let tx_submission_result = storage
            .transactions_dal()
            .insert_transaction_l2(new_tx, TransactionExecutionMetrics::default())
            .await;
        assert_matches!(tx_submission_result, L2TxSubmissionResult::Added);

        let new_miniblock = create_miniblock(1);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await?;
        Ok((new_miniblock, new_tx_hash))
    }

    async fn store_events(
        storage: &mut StorageProcessor<'_>,
        miniblock_number: u32,
        start_idx: u32,
    ) -> anyhow::Result<(IncludedTxLocation, Vec<VmEvent>)> {
        let new_miniblock = create_miniblock(miniblock_number);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await?;
        let tx_location = IncludedTxLocation {
            tx_hash: H256::repeat_byte(1),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::repeat_byte(2),
        };
        let events = vec![
            // Matches address, doesn't match topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx),
                address: Address::repeat_byte(23),
                indexed_topics: vec![],
                value: start_idx.to_le_bytes().to_vec(),
            },
            // Doesn't match address, matches topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 1),
                address: Address::zero(),
                indexed_topics: vec![H256::repeat_byte(42)],
                value: (start_idx + 1).to_le_bytes().to_vec(),
            },
            // Doesn't match address or topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 2),
                address: Address::zero(),
                indexed_topics: vec![H256::repeat_byte(1), H256::repeat_byte(42)],
                value: (start_idx + 2).to_le_bytes().to_vec(),
            },
            // Matches both address and topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 3),
                address: Address::repeat_byte(23),
                indexed_topics: vec![H256::repeat_byte(42), H256::repeat_byte(111)],
                value: (start_idx + 3).to_le_bytes().to_vec(),
            },
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 4),
                address: L2_ETH_TOKEN_ADDRESS,
                indexed_topics: vec![TRANSFER_EVENT_TOPIC],
                value: (start_idx + 4).to_le_bytes().to_vec(),
            },
            // ETH Transfer event with only topic matching
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 5),
                address: Address::repeat_byte(12),
                indexed_topics: vec![TRANSFER_EVENT_TOPIC],
                value: (start_idx + 5).to_le_bytes().to_vec(),
            },
            // ETH Transfer event with only address matching
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 6),
                address: L2_ETH_TOKEN_ADDRESS,
                indexed_topics: vec![H256::repeat_byte(25)],
                value: (start_idx + 6).to_le_bytes().to_vec(),
            },
        ];

        storage
            .events_dal()
            .save_events(
                MiniblockNumber(miniblock_number),
                &[(tx_location, events.iter().collect())],
            )
            .await;
        Ok((tx_location, events))
    }

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

            let l1_batch = create_l1_batch(number);
            storage
                .blocks_dal()
                .insert_l1_batch(&l1_batch, &[], BlockGasCount::default(), &[], &[], 0)
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

    async fn assert_migration(storage: &mut StorageProcessor<'_>) {
        for number in 0..5 {
            assert!(are_event_indexes_migrated(storage, MiniblockNumber(number))
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

        assert_eq!(result.events_affected, 5);

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

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn stopping_and_resuming_migration(chunk_size: u32) {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
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
        assert_migration(&mut storage).await;
    }

    #[test_casing(3, [1, 2, 3])]
    #[tokio::test]
    async fn new_blocks_added_during_migration(chunk_size: u32) {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
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
