use std::time::{Duration, Instant};

use tokio::sync::watch::Receiver;

use zksync_dal::ConnectionPool;
use zksync_types::aggregated_operations::AggregatedActionType;

use super::ActionQueue;

/// The task that keeps checking for the new batch status changes and persists them in the database.
pub fn run_batch_status_updater(
    pool: ConnectionPool,
    actions: ActionQueue,
    stop_receiver: Receiver<bool>,
) {
    loop {
        if *stop_receiver.borrow() {
            vlog::info!("Stop signal receiver, exiting the batch status updater routine");
            return;
        }

        let start = Instant::now();
        let mut storage = pool.access_storage_blocking();
        // Anything past this batch is not saved to the database.
        let last_sealed_batch = storage.blocks_dal().get_newest_block_header();

        let changes = actions.take_status_changes(last_sealed_batch.number);
        if changes.is_empty() {
            const DELAY_INTERVAL: Duration = Duration::from_secs(5);
            std::thread::sleep(DELAY_INTERVAL);
            continue;
        }

        for change in changes.commit.into_iter() {
            assert!(
                change.number <= last_sealed_batch.number,
                "Commit status change for the batch that is not sealed yet. Last sealed batch: {}, change: {:?}",
                last_sealed_batch.number,
                change
            );
            vlog::info!(
                "Commit status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            storage.eth_sender_dal().insert_bogus_confirmed_eth_tx(
                change.number,
                AggregatedActionType::CommitBlocks,
                change.l1_tx_hash,
                change.happened_at,
            );
        }
        for change in changes.prove.into_iter() {
            assert!(
                change.number <= last_sealed_batch.number,
                "Prove status change for the batch that is not sealed yet. Last sealed batch: {}, change: {:?}",
                last_sealed_batch.number,
                change
            );
            vlog::info!(
                "Prove status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );
            storage.eth_sender_dal().insert_bogus_confirmed_eth_tx(
                change.number,
                AggregatedActionType::PublishProofBlocksOnchain,
                change.l1_tx_hash,
                change.happened_at,
            );
        }
        for change in changes.execute.into_iter() {
            assert!(
                change.number <= last_sealed_batch.number,
                "Execute status change for the batch that is not sealed yet. Last sealed batch: {}, change: {:?}",
                last_sealed_batch.number,
                change
            );
            vlog::info!(
                "Execute status change: number {}, hash {}, happened at {}",
                change.number,
                change.l1_tx_hash,
                change.happened_at
            );

            storage.eth_sender_dal().insert_bogus_confirmed_eth_tx(
                change.number,
                AggregatedActionType::ExecuteBlocks,
                change.l1_tx_hash,
                change.happened_at,
            );
        }

        metrics::histogram!(
            "external_node.batch_status_updater.loop_iteration",
            start.elapsed()
        );
    }
}
