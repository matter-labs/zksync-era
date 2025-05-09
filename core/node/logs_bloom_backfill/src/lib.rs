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

            // We don't check the result: if a stop request is received, we'll return at the start
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
            return Ok(()); // Stop request received
        }

        let genesis_block_has_bloom = connection
            .blocks_dal()
            .has_l2_block_bloom(L2BlockNumber(0))
            .await?;
        if genesis_block_has_bloom {
            return Ok(()); // Migration has already been completed.
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
                tracing::info!("received a stop request; logs bloom backfill is shut down");
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

#[cfg(test)]
mod tests {
    use zksync_types::{
        block::L2BlockHeader, tx::IncludedTxLocation, Address, L1BatchNumber, H256,
    };
    use zksync_vm_interface::VmEvent;

    use super::*;

    async fn create_l2_block(
        conn: &mut Connection<'_, Core>,
        l2_block_number: L2BlockNumber,
        block_events: &[VmEvent],
    ) {
        let l2_block_header = L2BlockHeader {
            number: l2_block_number,
            timestamp: 0,
            hash: H256::from_low_u64_be(u64::from(l2_block_number.0)),
            l1_tx_count: 0,
            l2_tx_count: 0,
            fee_account_address: Address::repeat_byte(1),
            base_fee_per_gas: 0,
            gas_per_pubdata_limit: 0,
            batch_fee_input: Default::default(),
            base_system_contracts_hashes: Default::default(),
            protocol_version: Some(Default::default()),
            virtual_blocks: 0,
            gas_limit: 0,
            logs_bloom: Default::default(),
            pubdata_params: Default::default(),
        };

        conn.blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();

        let events_vec: Vec<_> = block_events.iter().collect();
        conn.events_dal()
            .save_events(
                l2_block_number,
                &[(
                    IncludedTxLocation {
                        tx_hash: Default::default(),
                        tx_index_in_l2_block: 0,
                    },
                    events_vec,
                )],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_logs_bloom_backfill() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut connection = connection_pool.connection().await.unwrap();
        connection
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&Default::default())
            .await
            .unwrap();

        let blocks_count = 5u32;
        for block_number in 0..blocks_count {
            let event = VmEvent {
                location: (L1BatchNumber(0), 0),
                address: Address::from_low_u64_be(block_number as u64 + 1),
                indexed_topics: Vec::new(),
                value: Vec::new(),
            };
            create_l2_block(&mut connection, L2BlockNumber(block_number), &[event]).await;

            if block_number + 1 < blocks_count {
                // Drop bloom if block is not last.
                connection
                    .blocks_dal()
                    .drop_l2_block_bloom(L2BlockNumber(block_number))
                    .await
                    .unwrap();
            }
        }
        let max_block_without_bloom = connection
            .blocks_dal()
            .get_max_l2_block_without_bloom()
            .await
            .unwrap();
        assert_eq!(
            max_block_without_bloom,
            Some(L2BlockNumber(blocks_count) - 2)
        );

        let migration = LogsBloomBackfill::new(connection_pool.clone());
        let (_sender, receiver) = watch::channel(false);
        migration.run(receiver).await.unwrap();

        for block_number in 0..(blocks_count - 1) {
            let header = connection
                .blocks_dal()
                .get_l2_block_header(L2BlockNumber(block_number))
                .await
                .unwrap()
                .unwrap();
            let address = Address::from_low_u64_be(block_number as u64 + 1);
            let contains_address = header
                .logs_bloom
                .contains_input(BloomInput::Raw(address.as_bytes()));
            assert!(contains_address);
        }
    }
}
