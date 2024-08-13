use std::time::Duration;

use anyhow::Context;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{block::build_bloom, BloomInput, L2BlockNumber};

#[derive(Debug)]
pub struct LogsBloomBackfill {
    connection_pool: ConnectionPool<Core>,
}

#[derive(Debug, PartialEq)]
enum BloomWaitOutcome {
    Ok,
    Canceled,
}

impl LogsBloomBackfill {
    pub fn new(connection_pool: ConnectionPool<Core>) -> Self {
        Self { connection_pool }
    }

    async fn wait_for_l2_block_with_bloom(
        connection: &mut Connection<'_, Core>,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<BloomWaitOutcome> {
        const INTERVAL: Duration = Duration::from_secs(1);
        tracing::debug!("waiting for at least one L2 block in DB with bloom");

        loop {
            if *stop_receiver.borrow() {
                return Ok(BloomWaitOutcome::Canceled);
            }

            if connection.blocks_dal().has_last_l2_block_bloom().await? {
                return Ok(BloomWaitOutcome::Ok);
            }

            // We don't check the result: if a stop signal is received, we'll return at the start
            // of the next iteration.
            tokio::time::timeout(INTERVAL, stop_receiver.changed())
                .await
                .ok();
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut connection = self
            .connection_pool
            .connection_tagged("logs_bloom_backfill")
            .await?;

        if Self::wait_for_l2_block_with_bloom(&mut connection, &mut stop_receiver).await?
            == BloomWaitOutcome::Canceled
        {
            return Ok(()); // Stop signal received
        }

        let max_block_without_bloom = connection
            .blocks_dal()
            .get_max_l2_block_without_bloom()
            .await?;
        let Some(max_block_without_bloom) = max_block_without_bloom else {
            tracing::info!("all blooms are already there, exiting migration");
            return Ok(());
        };
        let first_l2_block = connection
            .blocks_dal()
            .get_earliest_l2_block_number()
            .await?
            .context(
                "logs_bloom_backfill: missing l2 block in DB after waiting for at least one",
            )?;

        tracing::info!("starting blooms backfill from block {max_block_without_bloom}");
        let mut right_bound = max_block_without_bloom.0;
        loop {
            const WINDOW: u32 = 1000;

            if *stop_receiver.borrow_and_update() {
                tracing::info!("received a stop signal; logs bloom backfill is shut down");
            }

            let left_bound = right_bound.saturating_sub(WINDOW - 1).max(first_l2_block.0);
            tracing::info!(
                "started calculating blooms for block range {left_bound}..={right_bound}"
            );

            let mut bloom_items = connection
                .events_dal()
                .get_bloom_items_for_l2_blocks(
                    L2BlockNumber(left_bound)..=L2BlockNumber(right_bound),
                )
                .await?;

            let blooms: Vec<_> = (left_bound..=right_bound)
                .map(|block| {
                    let items = bloom_items
                        .remove(&L2BlockNumber(block))
                        .unwrap_or_default();
                    let iter = items.iter().map(|v| BloomInput::Raw(v.as_slice()));
                    build_bloom(iter)
                })
                .collect();
            connection
                .blocks_dal()
                .range_update_logs_bloom(L2BlockNumber(left_bound), &blooms)
                .await?;
            tracing::info!("filled blooms for block range {left_bound}..={right_bound}");

            if left_bound == first_l2_block.0 {
                break;
            } else {
                right_bound = left_bound - 1;
            }
        }

        tracing::info!("logs bloom backfill is finished");
        Ok(())
    }
}
