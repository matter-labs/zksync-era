use std::{collections::HashMap, fmt};

use sqlx::types::chrono::Utc;
use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::InstrumentExt, write_str, writeln_str,
};
use zksync_system_constants::L1_MESSENGER_ADDRESS;
use zksync_types::{
    api,
    event::L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    tx::IncludedTxLocation,
    Address, L1BatchNumber, MiniblockNumber, VmEvent, H256,
};

use crate::{
    models::storage_event::{StorageL2ToL1Log, StorageWeb3Log},
    Core, CoreDal, SqlxError,
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
    pub(crate) storage: &'a mut Connection<'c, Core>,
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
            r#"
            DELETE FROM events
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
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
            r#"
            DELETE FROM l2_to_l1_logs
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub(crate) async fn get_logs_by_tx_hashes(
        &mut self,
        hashes: &[H256],
    ) -> Result<HashMap<H256, Vec<api::Log>>, SqlxError> {
        let hashes = hashes
            .iter()
            .map(|hash| hash.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let logs: Vec<_> = sqlx::query_as!(
            StorageWeb3Log,
            r#"
            SELECT
                address,
                topic1,
                topic2,
                topic3,
                topic4,
                value,
                NULL::bytea AS "block_hash",
                NULL::BIGINT AS "l1_batch_number?",
                miniblock_number,
                tx_hash,
                tx_index_in_block,
                event_index_in_block,
                event_index_in_tx
            FROM
                events
            WHERE
                tx_hash = ANY ($1)
            ORDER BY
                miniblock_number ASC,
                event_index_in_block ASC
            "#,
            &hashes[..],
        )
        .fetch_all(self.storage.conn())
        .await?;

        let mut result = HashMap::<H256, Vec<api::Log>>::new();

        for storage_log in logs {
            let current_log = api::Log::from(storage_log);
            let tx_hash = current_log.transaction_hash.unwrap();
            result.entry(tx_hash).or_default().push(current_log);
        }

        Ok(result)
    }

    pub(crate) async fn get_l1_batch_raw_published_bytecode_hashes(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Vec<H256>> {
        let Some((from_miniblock, to_miniblock)) = self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await?
        else {
            return Ok(Vec::new());
        };

        let result: Vec<_> = sqlx::query!(
            r#"
            SELECT
                value
            FROM
                events
            WHERE
                miniblock_number BETWEEN $1 AND $2
                AND address = $3
                AND topic1 = $4
            ORDER BY
                miniblock_number,
                event_index_in_block
            "#,
            i64::from(from_miniblock.0),
            i64::from(to_miniblock.0),
            L1_MESSENGER_ADDRESS.as_bytes(),
            L1_MESSENGER_BYTECODE_PUBLICATION_EVENT_SIGNATURE.as_bytes()
        )
        .instrument("get_l1_batch_raw_published_bytecode_hashes")
        .with_arg("from_miniblock", &from_miniblock)
        .with_arg("to_miniblock", &to_miniblock)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| H256::from_slice(&row.value))
        .collect();

        Ok(result)
    }

    pub(crate) async fn get_l2_to_l1_logs_by_hashes(
        &mut self,
        hashes: &[H256],
    ) -> Result<HashMap<H256, Vec<api::L2ToL1Log>>, SqlxError> {
        let hashes = &hashes
            .iter()
            .map(|hash| hash.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let logs: Vec<_> = sqlx::query_as!(
            StorageL2ToL1Log,
            r#"
            SELECT
                miniblock_number,
                log_index_in_miniblock,
                log_index_in_tx,
                tx_hash,
                NULL::bytea AS "block_hash",
                NULL::BIGINT AS "l1_batch_number?",
                shard_id,
                is_service,
                tx_index_in_miniblock,
                tx_index_in_l1_batch,
                sender,
                key,
                value
            FROM
                l2_to_l1_logs
            WHERE
                tx_hash = ANY ($1)
            ORDER BY
                tx_index_in_l1_batch ASC,
                log_index_in_tx ASC
            "#,
            &hashes[..]
        )
        .fetch_all(self.storage.conn())
        .await?;

        let mut result = HashMap::<H256, Vec<api::L2ToL1Log>>::new();

        for storage_log in logs {
            let current_log = api::L2ToL1Log::from(storage_log);
            result
                .entry(current_log.transaction_hash)
                .or_default()
                .push(current_log);
        }

        Ok(result)
    }

    pub async fn get_vm_events_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<Vec<VmEvent>>> {
        let Some((from_miniblock, to_miniblock)) = self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await?
        else {
            return Ok(None);
        };

        let mut tx_index_in_l1_batch = -1;
        let rows = sqlx::query!(
            r#"
            SELECT
                address,
                topic1,
                topic2,
                topic3,
                topic4,
                value,
                event_index_in_tx
            FROM
                events
            WHERE
                miniblock_number BETWEEN $1 AND $2
            ORDER BY
                miniblock_number ASC,
                event_index_in_block ASC
            "#,
            i64::from(from_miniblock.0),
            i64::from(to_miniblock.0),
        )
        .instrument("get_vm_events_for_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                let indexed_topics = vec![row.topic1, row.topic2, row.topic3, row.topic4]
                    .into_iter()
                    .filter_map(|topic| {
                        if !topic.is_empty() {
                            Some(H256::from_slice(&topic))
                        } else {
                            None
                        }
                    })
                    .collect();
                if row.event_index_in_tx == 0 {
                    tx_index_in_l1_batch += 1;
                }
                VmEvent {
                    location: (l1_batch_number, tx_index_in_l1_batch as u32),
                    address: Address::from_slice(&row.address),
                    indexed_topics,
                    value: row.value,
                }
            })
            .collect();
        Ok(Some(events))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{Address, L1BatchNumber, ProtocolVersion};

    use super::*;
    use crate::{tests::create_miniblock_header, ConnectionPool, Core};

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
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
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
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
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
            .get_l2_to_l1_logs_by_hashes(&[H256([1; 32])])
            .await
            .unwrap();

        let logs = logs.get(&H256([1; 32])).unwrap().clone();

        assert_eq!(logs.len(), first_logs.len());
        for (i, log) in logs.iter().enumerate() {
            assert_eq!(log.log_index.as_usize(), i);
            assert_eq!(log.transaction_log_index.as_usize(), i);
        }
        for (log, expected_log) in logs.iter().zip(&first_logs) {
            assert_eq!(log.key.as_bytes(), expected_log.0.key.as_bytes());
            assert_eq!(log.value.as_bytes(), expected_log.0.value.as_bytes());
            assert_eq!(log.sender.as_bytes(), expected_log.0.sender.as_bytes());
        }

        let logs = conn
            .events_dal()
            .get_l2_to_l1_logs_by_hashes(&[H256([2; 32])])
            .await
            .unwrap()
            .get(&H256([2; 32]))
            .unwrap()
            .clone();

        assert_eq!(logs.len(), second_logs.len());
        for (i, log) in logs.iter().enumerate() {
            assert_eq!(log.log_index.as_usize(), i + first_logs.len());
            assert_eq!(log.transaction_log_index.as_usize(), i);
        }
        for (log, expected_log) in logs.iter().zip(&second_logs) {
            assert_eq!(log.key.as_bytes(), expected_log.0.key.as_bytes());
            assert_eq!(log.value.as_bytes(), expected_log.0.value.as_bytes());
            assert_eq!(log.sender.as_bytes(), expected_log.0.sender.as_bytes());
        }
    }
}
