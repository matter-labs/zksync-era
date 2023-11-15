use sqlx::types::chrono::Utc;

use std::fmt;

use crate::{models::storage_event::StorageL2ToL1Log, SqlxError, StorageProcessor};
use zksync_types::{
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    tx::IncludedTxLocation,
    MiniblockNumber, VmEvent, H256,
};

/// Wrapper around an optional event topic allowing to hex-format it for `COPY` instructions.
#[derive(Debug)]
struct EventTopic<'a>(Option<&'a H256>);

impl fmt::LowerHex for EventTopic<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(topic) = self.0 {
            fmt::LowerHex::fmt(topic, formatter)
        } else {
            Ok(()) // Don't write anything
        }
    }
}

#[derive(Debug)]
pub struct EventsDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl EventsDal<'_, '_> {
    /// Saves events for the specified miniblock.
    pub async fn save_events(
        &mut self,
        block_number: MiniblockNumber,
        all_block_events: &[(IncludedTxLocation, Vec<&VmEvent>)],
    ) {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY events(
                    miniblock_number, tx_hash, tx_index_in_block, address,
                    event_index_in_block, event_index_in_tx,
                    topic1, topic2, topic3, topic4, value,
                    tx_initiator_address,
                    created_at, updated_at
                )
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
        let mut event_index_in_block = 0_u32;
        for (tx_location, events) in all_block_events {
            let IncludedTxLocation {
                tx_hash,
                tx_index_in_miniblock,
                tx_initiator_address,
            } = tx_location;

            for (event_index_in_tx, event) in events.iter().enumerate() {
                write_str!(
                    &mut buffer,
                    r"{block_number}|\\x{tx_hash:x}|{tx_index_in_miniblock}|\\x{address:x}|",
                    address = event.address
                );
                write_str!(&mut buffer, "{event_index_in_block}|{event_index_in_tx}|");
                write_str!(
                    &mut buffer,
                    r"\\x{topic0:x}|\\x{topic1:x}|\\x{topic2:x}|\\x{topic3:x}|",
                    topic0 = EventTopic(event.indexed_topics.get(0)),
                    topic1 = EventTopic(event.indexed_topics.get(1)),
                    topic2 = EventTopic(event.indexed_topics.get(2)),
                    topic3 = EventTopic(event.indexed_topics.get(3))
                );
                writeln_str!(
                    &mut buffer,
                    r"\\x{value}|\\x{tx_initiator_address:x}|{now}|{now}",
                    value = hex::encode(&event.value)
                );

                event_index_in_block += 1;
            }
        }
        copy.send(buffer.as_bytes()).await.unwrap();
        // note: all the time spent in this function is spent in `copy.finish()`
        copy.finish().await.unwrap();
    }

    /// Removes events with a block number strictly greater than the specified `block_number`.
    pub async fn rollback_events(&mut self, block_number: MiniblockNumber) {
        sqlx::query!(
            "DELETE FROM events WHERE miniblock_number > $1",
            block_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    /// Saves user L2-to-L1 logs from a miniblock. Logs must be ordered by transaction location
    /// and within each transaction.
    pub async fn save_user_l2_to_l1_logs(
        &mut self,
        block_number: MiniblockNumber,
        all_block_l2_to_l1_logs: &[(IncludedTxLocation, Vec<&UserL2ToL1Log>)],
    ) {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY l2_to_l1_logs(
                    miniblock_number, log_index_in_miniblock, log_index_in_tx, tx_hash,
                    tx_index_in_miniblock, tx_index_in_l1_batch,
                    shard_id, is_service, sender, key, value,
                    created_at, updated_at
                )
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
        let mut log_index_in_miniblock = 0u32;
        for (tx_location, logs) in all_block_l2_to_l1_logs {
            let IncludedTxLocation {
                tx_hash,
                tx_index_in_miniblock,
                ..
            } = tx_location;

            for (log_index_in_tx, log) in logs.iter().enumerate() {
                let L2ToL1Log {
                    shard_id,
                    is_service,
                    tx_number_in_block,
                    sender,
                    key,
                    value,
                } = log.0;

                write_str!(
                    &mut buffer,
                    r"{block_number}|{log_index_in_miniblock}|{log_index_in_tx}|\\x{tx_hash:x}|"
                );
                write_str!(
                    &mut buffer,
                    r"{tx_index_in_miniblock}|{tx_number_in_block}|{shard_id}|{is_service}|"
                );
                writeln_str!(
                    &mut buffer,
                    r"\\x{sender:x}|\\x{key:x}|\\x{value:x}|{now}|{now}"
                );

                log_index_in_miniblock += 1;
            }
        }
        copy.send(buffer.as_bytes()).await.unwrap();
        copy.finish().await.unwrap();
    }

    /// Removes all L2-to-L1 logs with a miniblock number strictly greater than the specified `block_number`.
    pub async fn rollback_l2_to_l1_logs(&mut self, block_number: MiniblockNumber) {
        sqlx::query!(
            "DELETE FROM l2_to_l1_logs WHERE miniblock_number > $1",
            block_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub(crate) async fn l2_to_l1_logs(
        &mut self,
        tx_hash: H256,
    ) -> Result<Vec<StorageL2ToL1Log>, SqlxError> {
        sqlx::query_as!(
            StorageL2ToL1Log,
            "SELECT \
                miniblock_number, log_index_in_miniblock, log_index_in_tx, tx_hash, \
                Null::bytea as \"block_hash\", Null::bigint as \"l1_batch_number?\", \
                shard_id, is_service, tx_index_in_miniblock, tx_index_in_l1_batch, sender, key, value \
            FROM l2_to_l1_logs \
            WHERE tx_hash = $1 \
            ORDER BY log_index_in_tx ASC",
            tx_hash.as_bytes()
        )
        .fetch_all(self.storage.conn())
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tests::create_miniblock_header, ConnectionPool};
    use zksync_types::{Address, L1BatchNumber, ProtocolVersion};

    fn create_vm_event(index: u8, topic_count: u8) -> VmEvent {
        assert!(topic_count <= 4);
        VmEvent {
            location: (L1BatchNumber(1), u32::from(index)),
            address: Address::repeat_byte(index),
            indexed_topics: (0..topic_count).map(H256::repeat_byte).collect(),
            value: vec![index],
        }
    }

    #[tokio::test]
    async fn storing_events() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.events_dal().rollback_events(MiniblockNumber(0)).await;
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(1))
            .await
            .unwrap();

        let first_location = IncludedTxLocation {
            tx_hash: H256([1; 32]),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::default(),
        };
        let first_events = vec![create_vm_event(0, 0), create_vm_event(1, 4)];
        let second_location = IncludedTxLocation {
            tx_hash: H256([2; 32]),
            tx_index_in_miniblock: 1,
            tx_initiator_address: Address::default(),
        };
        let second_events = vec![
            create_vm_event(2, 2),
            create_vm_event(3, 3),
            create_vm_event(4, 4),
        ];
        let all_events = vec![
            (first_location, first_events.iter().collect()),
            (second_location, second_events.iter().collect()),
        ];
        conn.events_dal()
            .save_events(MiniblockNumber(1), &all_events)
            .await;

        let logs = conn
            .events_web3_dal()
            .get_all_logs(MiniblockNumber(0))
            .await
            .unwrap();
        assert_eq!(logs.len(), 5);
        for (i, log) in logs.iter().enumerate() {
            let (expected_tx_index, expected_topics) = if i < first_events.len() {
                (0_u64, &first_events[i].indexed_topics)
            } else {
                (1_u64, &second_events[i - first_events.len()].indexed_topics)
            };
            let i = i as u8;

            assert_eq!(log.block_number, Some(1_u64.into()));
            assert_eq!(log.l1_batch_number, None);
            assert_eq!(log.address, Address::repeat_byte(i));
            assert_eq!(log.transaction_index, Some(expected_tx_index.into()));
            assert_eq!(log.log_index, Some(i.into()));
            assert_eq!(log.data.0, [i]);
            assert_eq!(log.topics, *expected_topics);
        }
    }

    fn create_l2_to_l1_log(tx_number_in_block: u16, index: u8) -> UserL2ToL1Log {
        UserL2ToL1Log(L2ToL1Log {
            shard_id: 0,
            is_service: false,
            tx_number_in_block,
            sender: Address::repeat_byte(index),
            key: H256::from_low_u64_be(u64::from(index)),
            value: H256::repeat_byte(index),
        })
    }

    #[tokio::test]
    async fn storing_l2_to_l1_logs() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.events_dal()
            .rollback_l2_to_l1_logs(MiniblockNumber(0))
            .await;
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(1))
            .await
            .unwrap();

        let first_location = IncludedTxLocation {
            tx_hash: H256([1; 32]),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::default(),
        };
        let first_logs = vec![create_l2_to_l1_log(0, 0), create_l2_to_l1_log(0, 1)];
        let second_location = IncludedTxLocation {
            tx_hash: H256([2; 32]),
            tx_index_in_miniblock: 1,
            tx_initiator_address: Address::default(),
        };
        let second_logs = vec![
            create_l2_to_l1_log(1, 2),
            create_l2_to_l1_log(1, 3),
            create_l2_to_l1_log(1, 4),
        ];
        let all_logs = vec![
            (first_location, first_logs.iter().collect()),
            (second_location, second_logs.iter().collect()),
        ];
        conn.events_dal()
            .save_user_l2_to_l1_logs(MiniblockNumber(1), &all_logs)
            .await;

        let logs = conn
            .events_dal()
            .l2_to_l1_logs(H256([1; 32]))
            .await
            .unwrap();
        assert_eq!(logs.len(), first_logs.len());
        for (i, log) in logs.iter().enumerate() {
            assert_eq!(log.log_index_in_miniblock as usize, i);
            assert_eq!(log.log_index_in_tx as usize, i);
        }
        for (log, expected_log) in logs.iter().zip(&first_logs) {
            assert_eq!(log.key, expected_log.0.key.as_bytes());
            assert_eq!(log.value, expected_log.0.value.as_bytes());
            assert_eq!(log.sender, expected_log.0.sender.as_bytes());
        }

        let logs = conn
            .events_dal()
            .l2_to_l1_logs(H256([2; 32]))
            .await
            .unwrap();
        assert_eq!(logs.len(), second_logs.len());
        for (i, log) in logs.iter().enumerate() {
            assert_eq!(log.log_index_in_miniblock as usize, i + first_logs.len());
            assert_eq!(log.log_index_in_tx as usize, i);
        }
        for (log, expected_log) in logs.iter().zip(&second_logs) {
            assert_eq!(log.key, expected_log.0.key.as_bytes());
            assert_eq!(log.value, expected_log.0.value.as_bytes());
            assert_eq!(log.sender, expected_log.0.sender.as_bytes());
        }
    }
}
