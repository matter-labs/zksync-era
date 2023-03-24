use crate::StorageProcessor;
use sqlx::types::chrono::Utc;
use zksync_types::l2_to_l1_log::L2ToL1Log;
use zksync_types::{tx::IncludedTxLocation, MiniblockNumber, VmEvent};

#[derive(Debug)]
pub struct EventsDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl EventsDal<'_, '_> {
    pub fn save_events(
        &mut self,
        block_number: MiniblockNumber,
        all_block_events: Vec<(IncludedTxLocation, Vec<VmEvent>)>,
    ) {
        async_std::task::block_on(async {
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

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            let mut event_index_in_block = 0u32;
            let mut event_index_in_tx: u32;
            for (
                IncludedTxLocation {
                    tx_hash,
                    tx_index_in_miniblock: tx_index_in_block,
                    tx_initiator_address,
                },
                events,
            ) in all_block_events
            {
                event_index_in_tx = 0;
                let tx_hash_str = format!("\\\\x{}", hex::encode(tx_hash.0));
                let tx_initiator_address_str =
                    format!("\\\\x{}", hex::encode(tx_initiator_address.0));
                for event in events {
                    let address_str = format!("\\\\x{}", hex::encode(event.address.0));
                    let mut topics_str: Vec<String> = event
                        .indexed_topics
                        .into_iter()
                        .map(|topic| format!("\\\\x{}", hex::encode(topic.0)))
                        .collect();
                    topics_str.resize(4, "\\\\x".to_string());
                    let value_str = format!("\\\\x{}", hex::encode(event.value));
                    let row = format!(
                        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                        block_number,
                        tx_hash_str,
                        tx_index_in_block,
                        address_str,
                        event_index_in_block,
                        event_index_in_tx,
                        topics_str[0],
                        topics_str[1],
                        topics_str[2],
                        topics_str[3],
                        value_str,
                        tx_initiator_address_str,
                        now,
                        now
                    );
                    bytes.extend_from_slice(row.as_bytes());

                    event_index_in_block += 1;
                    event_index_in_tx += 1;
                }
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        })
    }

    pub fn rollback_events(&mut self, block_number: MiniblockNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM events WHERE miniblock_number > $1",
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn save_l2_to_l1_logs(
        &mut self,
        block_number: MiniblockNumber,
        all_block_l2_to_l1_logs: Vec<(IncludedTxLocation, Vec<L2ToL1Log>)>,
    ) {
        async_std::task::block_on(async {
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

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            let mut log_index_in_miniblock = 0u32;
            let mut log_index_in_tx: u32;
            for (tx_location, logs) in all_block_l2_to_l1_logs {
                log_index_in_tx = 0;
                let tx_hash_str = format!("\\\\x{}", hex::encode(tx_location.tx_hash.0));
                for log in logs {
                    let sender_str = format!("\\\\x{}", hex::encode(log.sender));
                    let key_str = format!("\\\\x{}", hex::encode(log.key));
                    let value_str = format!("\\\\x{}", hex::encode(log.value));
                    let row = format!(
                        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                        block_number,
                        log_index_in_miniblock,
                        log_index_in_tx,
                        tx_hash_str,
                        tx_location.tx_index_in_miniblock,
                        log.tx_number_in_block,
                        log.shard_id,
                        log.is_service,
                        sender_str,
                        key_str,
                        value_str,
                        now,
                        now
                    );
                    bytes.extend_from_slice(row.as_bytes());

                    log_index_in_miniblock += 1;
                    log_index_in_tx += 1;
                }
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        })
    }

    pub fn rollback_l2_to_l1_logs(&mut self, block_number: MiniblockNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM l2_to_l1_logs WHERE miniblock_number > $1",
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_first_miniblock_with_saved_l2_to_l1_logs(&mut self) -> Option<MiniblockNumber> {
        async_std::task::block_on(async {
            let row = sqlx::query!(
                r#"
                    SELECT MIN(miniblock_number) as "min?"
                    FROM l2_to_l1_logs
                "#,
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();
            row.min.map(|min| MiniblockNumber(min as u32))
        })
    }
}
