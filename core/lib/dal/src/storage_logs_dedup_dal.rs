use crate::StorageProcessor;
use sqlx::types::chrono::Utc;
use std::collections::{HashMap, HashSet};
use vm::zk_evm::aux_structures::LogQuery;
use zksync_types::{AccountTreeId, Address, L1BatchNumber, StorageKey, H256};
use zksync_utils::u256_to_h256;

#[derive(Debug)]
pub struct StorageLogsDedupDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl StorageLogsDedupDal<'_, '_> {
    pub fn insert_storage_logs(&mut self, block_number: L1BatchNumber, logs: &[LogQuery]) {
        async_std::task::block_on(async {
            let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY storage_logs_dedup (hashed_key, address, key, value_read, value_written, operation_number, is_write, l1_batch_number, created_at)
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            for (operation_number, log) in logs.iter().enumerate() {
                let hashed_key_str = format!(
                    "\\\\x{}",
                    hex::encode(StorageKey::raw_hashed_key(
                        &log.address,
                        &u256_to_h256(log.key)
                    ))
                );
                let address_str = format!("\\\\x{}", hex::encode(log.address.0));
                let key_str = format!("\\\\x{}", hex::encode(u256_to_h256(log.key).0));
                let read_value_str =
                    format!("\\\\x{}", hex::encode(u256_to_h256(log.read_value).0));
                let written_value_str =
                    format!("\\\\x{}", hex::encode(u256_to_h256(log.written_value).0));
                let row = format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                    hashed_key_str,
                    address_str,
                    key_str,
                    read_value_str,
                    written_value_str,
                    operation_number,
                    log.rw_flag,
                    block_number,
                    now
                );
                bytes.extend_from_slice(row.as_bytes());
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        })
    }

    pub fn rollback_storage_logs(&mut self, block_number: L1BatchNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM storage_logs_dedup WHERE l1_batch_number > $1",
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn insert_protective_reads(
        &mut self,
        l1_batch_number: L1BatchNumber,
        read_logs: &[LogQuery],
    ) {
        async_std::task::block_on(async {
            let mut copy = self
                .storage
                .conn()
                .copy_in_raw(
                    "COPY protective_reads (l1_batch_number, address, key, created_at, updated_at)
                    FROM STDIN WITH (DELIMITER '|')",
                )
                .await
                .unwrap();

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            for log in read_logs.iter() {
                let address_str = format!("\\\\x{}", hex::encode(log.address.0));
                let key_str = format!("\\\\x{}", hex::encode(u256_to_h256(log.key).0));
                let row = format!(
                    "{}|{}|{}|{}|{}\n",
                    l1_batch_number, address_str, key_str, now, now
                );
                bytes.extend_from_slice(row.as_bytes());
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        })
    }

    pub fn insert_initial_writes(
        &mut self,
        l1_batch_number: L1BatchNumber,
        write_logs: &[LogQuery],
    ) {
        async_std::task::block_on(async {
            let hashed_keys: Vec<_> = write_logs
                .iter()
                .map(|log| {
                    StorageKey::raw_hashed_key(&log.address, &u256_to_h256(log.key)).to_vec()
                })
                .collect();

            sqlx::query!(
                "INSERT INTO initial_writes (hashed_key, l1_batch_number, created_at, updated_at)
                SELECT u.hashed_key, $2, now(), now()
                FROM UNNEST($1::bytea[]) AS u(hashed_key)
                ON CONFLICT (hashed_key) DO NOTHING
                ",
                &hashed_keys,
                l1_batch_number.0 as i64,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_protective_reads_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashSet<StorageKey> {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                SELECT address, key FROM protective_reads
                WHERE l1_batch_number = $1
                ",
                l1_batch_number.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                StorageKey::new(
                    AccountTreeId::new(Address::from_slice(&row.address)),
                    H256::from_slice(&row.key),
                )
            })
            .collect()
        })
    }

    pub fn get_touched_slots_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashMap<StorageKey, H256> {
        async_std::task::block_on(async {
            let storage_logs = sqlx::query!(
                "
                SELECT address, key, value
                FROM storage_logs
                WHERE miniblock_number BETWEEN (SELECT MIN(number) FROM miniblocks WHERE l1_batch_number = $1)
                    AND (SELECT MAX(number) FROM miniblocks WHERE l1_batch_number = $1)
                ORDER BY miniblock_number, operation_number
                ",
                l1_batch_number.0 as i64
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap();

            let mut touched_slots = HashMap::new();
            for storage_log in storage_logs.into_iter() {
                touched_slots.insert(
                    StorageKey::new(
                        AccountTreeId::new(Address::from_slice(&storage_log.address)),
                        H256::from_slice(&storage_log.key),
                    ),
                    H256::from_slice(&storage_log.value),
                );
            }
            touched_slots
        })
    }

    pub fn get_storage_logs_for_revert(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Vec<(H256, Option<H256>)> {
        async_std::task::block_on(async {
            let miniblock_number = match self
                .storage
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(l1_batch_number)
            {
                None => return Vec::new(),
                Some((_, number)) => number,
            };

            vlog::info!("fetching keys that were changed after given block number");
            let modified_keys: Vec<H256> = sqlx::query!(
                "SELECT DISTINCT ON (hashed_key) hashed_key FROM
                (SELECT * FROM storage_logs WHERE miniblock_number > $1) inn",
                miniblock_number.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| H256::from_slice(&row.hashed_key))
            .collect();
            vlog::info!("loaded {:?} keys", modified_keys.len());

            let mut result: Vec<(H256, Option<H256>)> = vec![];

            for key in modified_keys {
                let initially_written_at: Option<L1BatchNumber> = sqlx::query!(
                    "
                        SELECT l1_batch_number FROM initial_writes
                        WHERE hashed_key = $1
                    ",
                    key.as_bytes(),
                )
                .fetch_optional(self.storage.conn())
                .await
                .unwrap()
                .map(|row| L1BatchNumber(row.l1_batch_number as u32));
                match initially_written_at {
                    // Key isn't written to the storage - nothing to rollback.
                    None => continue,
                    // Key was initially written, it's needed to remove it.
                    Some(initially_written_at) if initially_written_at > l1_batch_number => {
                        result.push((key, None));
                    }
                    // Key was rewritten, it's needed to restore the previous value.
                    Some(_) => {
                        let previous_value: Vec<u8> = sqlx::query!(
                            "
                            SELECT value FROM storage_logs
                            WHERE hashed_key = $1 AND miniblock_number <= $2
                            ORDER BY miniblock_number DESC, operation_number DESC
                            LIMIT 1
                            ",
                            key.as_bytes(),
                            miniblock_number.0 as i64
                        )
                        .fetch_one(self.storage.conn())
                        .await
                        .unwrap()
                        .value;
                        result.push((key, Some(H256::from_slice(&previous_value))));
                    }
                }
                if result.len() % 1000 == 0 {
                    vlog::info!("processed {:?} values", result.len());
                }
            }

            result
        })
    }

    pub fn get_previous_storage_values(
        &mut self,
        hashed_keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> HashMap<H256, H256> {
        async_std::task::block_on(async {
            let hashed_keys: Vec<_> = hashed_keys.into_iter().map(|key| key.0.to_vec()).collect();
            let (miniblock_number, _) = self
                .storage
                .blocks_dal()
                .get_miniblock_range_of_l1_batch(l1_batch_number)
                .unwrap();
            sqlx::query!(
                r#"
                    SELECT u.hashed_key as "hashed_key!",
                        (SELECT value FROM storage_logs
                        WHERE hashed_key = u.hashed_key AND miniblock_number < $2
                        ORDER BY miniblock_number DESC, operation_number DESC LIMIT 1) as "value?"
                    FROM UNNEST($1::bytea[]) AS u(hashed_key)
                "#,
                &hashed_keys,
                miniblock_number.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    H256::from_slice(&row.hashed_key),
                    row.value
                        .map(|value| H256::from_slice(&value))
                        .unwrap_or_else(H256::zero),
                )
            })
            .collect()
        })
    }
}
