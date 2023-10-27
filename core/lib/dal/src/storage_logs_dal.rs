use sqlx::types::chrono::Utc;
use sqlx::Row;

use std::{collections::HashMap, time::Instant};

use crate::{instrument::InstrumentExt, StorageProcessor};
use zksync_types::{
    get_code_key, AccountTreeId, Address, L1BatchNumber, MiniblockNumber, StorageKey, StorageLog,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H256,
};

#[derive(Debug)]
pub struct StorageLogsDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl StorageLogsDal<'_, '_> {
    /// Inserts storage logs grouped by transaction for a miniblock. The ordering of transactions
    /// must be the same as their ordering in the miniblock.
    pub async fn insert_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
    ) {
        self.insert_storage_logs_inner(block_number, logs, 0).await;
    }

    async fn insert_storage_logs_inner(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
        mut operation_number: u32,
    ) {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY storage_logs(
                    hashed_key, address, key, value, operation_number, tx_hash, miniblock_number,
                    created_at, updated_at
                )
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
        for (tx_hash, logs) in logs {
            for log in logs {
                write_str!(
                    &mut buffer,
                    r"\\x{hashed_key:x}|\\x{address:x}|\\x{key:x}|\\x{value:x}|",
                    hashed_key = log.key.hashed_key(),
                    address = log.key.address(),
                    key = log.key.key(),
                    value = log.value
                );
                writeln_str!(
                    &mut buffer,
                    r"{operation_number}|\\x{tx_hash:x}|{block_number}|{now}|{now}"
                );

                operation_number += 1;
            }
        }
        copy.send(buffer.as_bytes()).await.unwrap();
        copy.finish().await.unwrap();
    }

    pub async fn append_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
    ) {
        let operation_number = sqlx::query!(
            "SELECT MAX(operation_number) as \"max?\" FROM storage_logs WHERE miniblock_number = $1",
            block_number.0 as i64
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .max
        .map(|max| max as u32 + 1)
        .unwrap_or(0);

        self.insert_storage_logs_inner(block_number, logs, operation_number)
            .await;
    }

    /// Rolls back storage to the specified point in time.
    pub async fn rollback_storage(&mut self, last_miniblock_to_keep: MiniblockNumber) {
        let stage_start = Instant::now();
        let modified_keys = self
            .modified_keys_since_miniblock(last_miniblock_to_keep)
            .await;
        tracing::info!(
            "Loaded {} keys changed after miniblock #{last_miniblock_to_keep} in {:?}",
            modified_keys.len(),
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        let prev_values = self
            .get_storage_values(&modified_keys, last_miniblock_to_keep)
            .await;
        tracing::info!(
            "Loaded previous storage values for modified keys in {:?}",
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        let mut keys_to_delete = vec![];
        let mut keys_to_update = vec![];
        let mut values_to_update = vec![];
        for (key, maybe_value) in &prev_values {
            if let Some(prev_value) = maybe_value {
                keys_to_update.push(key.as_bytes());
                values_to_update.push(prev_value.as_bytes());
            } else {
                keys_to_delete.push(key.as_bytes());
            }
        }
        tracing::info!(
            "Created revert plan (keys to update: {}, to delete: {}) in {:?}",
            keys_to_update.len(),
            keys_to_delete.len(),
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        sqlx::query!(
            "DELETE FROM storage WHERE hashed_key = ANY($1)",
            &keys_to_delete as &[&[u8]],
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
        tracing::info!(
            "Removed {} keys in {:?}",
            keys_to_delete.len(),
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        sqlx::query!(
            "UPDATE storage SET value = u.value \
            FROM UNNEST($1::bytea[], $2::bytea[]) AS u(key, value) \
            WHERE u.key = hashed_key",
            &keys_to_update as &[&[u8]],
            &values_to_update as &[&[u8]],
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
        tracing::info!(
            "Updated {} keys to previous values in {:?}",
            keys_to_update.len(),
            stage_start.elapsed()
        );
    }

    /// Returns all storage keys that were modified after the specified miniblock.
    async fn modified_keys_since_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Vec<H256> {
        sqlx::query!(
            "SELECT DISTINCT ON (hashed_key) hashed_key FROM \
            (SELECT * FROM storage_logs WHERE miniblock_number > $1) inn",
            miniblock_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| H256::from_slice(&row.hashed_key))
        .collect()
    }

    /// Removes all storage logs with a miniblock number strictly greater than the specified `block_number`.
    pub async fn rollback_storage_logs(&mut self, block_number: MiniblockNumber) {
        sqlx::query!(
            "DELETE FROM storage_logs WHERE miniblock_number > $1",
            block_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn is_contract_deployed_at_address(&mut self, address: Address) -> bool {
        let hashed_key = get_code_key(&address).hashed_key();
        let row = sqlx::query!(
            "SELECT COUNT(*) as \"count!\" \
            FROM (\
                SELECT * FROM storage_logs \
                WHERE storage_logs.hashed_key = $1 \
                ORDER BY storage_logs.miniblock_number DESC, storage_logs.operation_number DESC \
                LIMIT 1\
            ) sl \
            WHERE sl.value != $2",
            hashed_key.as_bytes(),
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        row.count > 0
    }

    /// Returns latest values for all [`StorageKey`]s written to in the specified L1 batch
    /// judging by storage logs (i.e., not taking deduplication logic into account).
    pub async fn get_touched_slots_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashMap<StorageKey, H256> {
        let rows = sqlx::query!(
            "SELECT address, key, value \
            FROM storage_logs \
            WHERE miniblock_number BETWEEN \
                (SELECT MIN(number) FROM miniblocks WHERE l1_batch_number = $1) \
                AND (SELECT MAX(number) FROM miniblocks WHERE l1_batch_number = $1) \
            ORDER BY miniblock_number, operation_number",
            l1_batch_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        let touched_slots = rows.into_iter().map(|row| {
            let key = StorageKey::new(
                AccountTreeId::new(Address::from_slice(&row.address)),
                H256::from_slice(&row.key),
            );
            (key, H256::from_slice(&row.value))
        });
        touched_slots.collect()
    }

    /// Returns (hashed) storage keys and the corresponding values that need to be applied to a storage
    /// in order to revert it to the specified L1 batch. Deduplication is taken into account.
    pub async fn get_storage_logs_for_revert(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashMap<H256, Option<(H256, u64)>> {
        let miniblock_range = self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await
            .unwrap();
        let Some((_, last_miniblock)) = miniblock_range else {
            return HashMap::new();
        };

        let stage_start = Instant::now();
        let mut modified_keys = self.modified_keys_since_miniblock(last_miniblock).await;
        let modified_keys_count = modified_keys.len();
        tracing::info!(
            "Fetched {modified_keys_count} keys changed after miniblock #{last_miniblock} in {:?}",
            stage_start.elapsed()
        );

        // We need to filter `modified_keys` using the `initial_writes` table (i.e., take dedup logic
        // into account). Some keys that have `storage_logs` entries are actually never written to
        // as per `initial_writes`, so if we return such keys from this method, it will lead to
        // the incorrect state after revert.
        let stage_start = Instant::now();
        let l1_batch_and_index_by_key = self
            .get_l1_batches_and_indices_for_initial_writes(&modified_keys)
            .await;
        tracing::info!(
            "Loaded initial write info for modified keys in {:?}",
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        let mut output = HashMap::with_capacity(modified_keys.len());
        modified_keys.retain(|key| {
            match l1_batch_and_index_by_key.get(key) {
                None => {
                    // Key is completely deduped. It should not be present in the output map.
                    false
                }
                Some((write_batch, _)) if *write_batch > l1_batch_number => {
                    // Key was initially written to after the specified L1 batch.
                    output.insert(*key, None);
                    false
                }
                Some(_) => true,
            }
        });
        tracing::info!(
            "Filtered modified keys per initial writes in {:?}",
            stage_start.elapsed()
        );

        let deduped_count = modified_keys_count - l1_batch_and_index_by_key.len();
        tracing::info!(
            "Keys to update: {update_count}, to delete: {delete_count}; {deduped_count} modified keys \
             are deduped and will be ignored",
            update_count = modified_keys.len(),
            delete_count = l1_batch_and_index_by_key.len() - modified_keys.len()
        );

        let stage_start = Instant::now();
        let prev_values_for_updated_keys = self
            .get_storage_values(&modified_keys, last_miniblock)
            .await
            .into_iter()
            .map(|(key, value)| {
                let value = value.unwrap(); // We already filtered out keys that weren't touched.
                let index = l1_batch_and_index_by_key[&key].1;
                (key, Some((value, index)))
            });
        tracing::info!(
            "Loaded previous values for {} keys in {:?}",
            prev_values_for_updated_keys.len(),
            stage_start.elapsed()
        );
        output.extend(prev_values_for_updated_keys);
        output
    }

    pub async fn get_l1_batches_and_indices_for_initial_writes(
        &mut self,
        hashed_keys: &[H256],
    ) -> HashMap<H256, (L1BatchNumber, u64)> {
        if hashed_keys.is_empty() {
            return HashMap::new(); // Shortcut to save time on communication with DB in the common case
        }

        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();
        let rows = sqlx::query!(
            "SELECT hashed_key, l1_batch_number, index FROM initial_writes \
            WHERE hashed_key = ANY($1::bytea[])",
            &hashed_keys as &[&[u8]],
        )
        .instrument("get_l1_batches_and_indices_for_initial_writes")
        .report_latency()
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        rows.into_iter()
            .map(|row| {
                (
                    H256::from_slice(&row.hashed_key),
                    (L1BatchNumber(row.l1_batch_number as u32), row.index as u64),
                )
            })
            .collect()
    }

    /// Gets previous values for the specified storage keys before the specified L1 batch number.
    ///
    /// # Return value
    ///
    /// The returned map is guaranteed to contain all unique keys from `hashed_keys`.
    ///
    /// # Performance
    ///
    /// This DB query is slow, especially when used with large `hashed_keys` slices. Prefer using alternatives
    /// wherever possible.
    pub async fn get_previous_storage_values(
        &mut self,
        hashed_keys: &[H256],
        next_l1_batch: L1BatchNumber,
    ) -> HashMap<H256, Option<H256>> {
        let (miniblock_number, _) = self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(next_l1_batch)
            .await
            .unwrap()
            .unwrap();

        if miniblock_number == MiniblockNumber(0) {
            hashed_keys.iter().copied().map(|key| (key, None)).collect()
        } else {
            self.get_storage_values(hashed_keys, miniblock_number - 1)
                .await
        }
    }

    /// Returns current values for the specified keys at the specified `miniblock_number`.
    pub async fn get_storage_values(
        &mut self,
        hashed_keys: &[H256],
        miniblock_number: MiniblockNumber,
    ) -> HashMap<H256, Option<H256>> {
        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();

        let rows = sqlx::query!(
            "SELECT u.hashed_key as \"hashed_key!\", \
                (SELECT value FROM storage_logs \
                WHERE hashed_key = u.hashed_key AND miniblock_number <= $2 \
                ORDER BY miniblock_number DESC, operation_number DESC LIMIT 1) as \"value?\" \
            FROM UNNEST($1::bytea[]) AS u(hashed_key)",
            &hashed_keys as &[&[u8]],
            miniblock_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        rows.into_iter()
            .map(|row| {
                let key = H256::from_slice(&row.hashed_key);
                let value = row.value.map(|value| H256::from_slice(&value));
                (key, value)
            })
            .collect()
    }

    /// Resolves hashed keys into storage keys ((address, key) tuples).
    /// Panics if there is an unknown hashed key in the input.
    pub async fn resolve_hashed_keys(&mut self, hashed_keys: &[H256]) -> Vec<StorageKey> {
        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();
        sqlx::query!(
            "SELECT \
                (SELECT ARRAY[address,key] FROM storage_logs \
                WHERE hashed_key = u.hashed_key \
                ORDER BY miniblock_number, operation_number \
                LIMIT 1) as \"address_and_key?\" \
            FROM UNNEST($1::bytea[]) AS u(hashed_key)",
            &hashed_keys as &[&[u8]],
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            let address_and_key = row.address_and_key.unwrap();
            StorageKey::new(
                AccountTreeId::new(Address::from_slice(&address_and_key[0])),
                H256::from_slice(&address_and_key[1]),
            )
        })
        .collect()
    }

    pub async fn get_miniblock_storage_logs(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Vec<(H256, H256, u32)> {
        self.get_miniblock_storage_logs_from_table(miniblock_number, "storage_logs")
            .await
    }

    pub async fn retain_storage_logs(
        &mut self,
        miniblock_number: MiniblockNumber,
        operation_numbers: &[i32],
    ) {
        sqlx::query!(
            "DELETE FROM storage_logs \
            WHERE miniblock_number = $1 AND operation_number != ALL($2)",
            miniblock_number.0 as i64,
            &operation_numbers
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    /// Loads (hashed_key, value, operation_number) tuples for given miniblock_number.
    /// Uses provided DB table.
    /// Shouldn't be used in production.
    pub async fn get_miniblock_storage_logs_from_table(
        &mut self,
        miniblock_number: MiniblockNumber,
        table_name: &str,
    ) -> Vec<(H256, H256, u32)> {
        sqlx::query(&format!(
            "SELECT hashed_key, value, operation_number FROM {table_name} \
            WHERE miniblock_number = $1 \
            ORDER BY operation_number"
        ))
        .bind(miniblock_number.0 as i64)
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            let hashed_key = H256::from_slice(row.get("hashed_key"));
            let value = H256::from_slice(row.get("value"));
            let operation_number: u32 = row.get::<i32, &str>("operation_number") as u32;
            (hashed_key, value, operation_number)
        })
        .collect()
    }

    /// Loads value for given hashed_key at given miniblock_number.
    /// Uses provided DB table.
    /// Shouldn't be used in production.
    pub async fn get_storage_value_from_table(
        &mut self,
        hashed_key: H256,
        miniblock_number: MiniblockNumber,
        table_name: &str,
    ) -> H256 {
        let query_str = format!(
            "SELECT value FROM {table_name} \
                WHERE hashed_key = $1 AND miniblock_number <= $2 \
                ORDER BY miniblock_number DESC, operation_number DESC LIMIT 1",
        );
        sqlx::query(&query_str)
            .bind(hashed_key.as_bytes())
            .bind(miniblock_number.0 as i64)
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|row| H256::from_slice(row.get("value")))
            .unwrap_or_else(H256::zero)
    }

    /// Vacuums `storage_logs` table.
    /// Shouldn't be used in production.
    pub async fn vacuum_storage_logs(&mut self) {
        sqlx::query!("VACUUM storage_logs")
            .execute(self.storage.conn())
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tests::create_miniblock_header, ConnectionPool};
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::{BlockGasCount, L1BatchHeader},
        ProtocolVersion, ProtocolVersionId,
    };

    async fn insert_miniblock(conn: &mut StorageProcessor<'_>, number: u32, logs: Vec<StorageLog>) {
        let mut header = L1BatchHeader::new(
            L1BatchNumber(number),
            0,
            Address::default(),
            BaseSystemContractsHashes::default(),
            ProtocolVersionId::default(),
        );
        header.is_finished = true;
        conn.blocks_dal()
            .insert_l1_batch(&header, &[], BlockGasCount::default(), &[], &[])
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(number))
            .await
            .unwrap();

        let logs = [(H256::zero(), logs)];
        conn.storage_logs_dal()
            .insert_storage_logs(MiniblockNumber(number), &logs)
            .await;
        conn.storage_dal().apply_storage_logs(&logs).await;
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(number))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn inserting_storage_logs() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();

        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.blocks_dal()
            .delete_l1_batches(L1BatchNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let first_key = StorageKey::new(account, H256::zero());
        let second_key = StorageKey::new(account, H256::from_low_u64_be(1));
        let log = StorageLog::new_write_log(first_key, H256::repeat_byte(1));
        let other_log = StorageLog::new_write_log(second_key, H256::repeat_byte(2));
        insert_miniblock(&mut conn, 1, vec![log, other_log]).await;

        let touched_slots = conn
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(L1BatchNumber(1))
            .await;
        assert_eq!(touched_slots.len(), 2);
        assert_eq!(touched_slots[&first_key], H256::repeat_byte(1));
        assert_eq!(touched_slots[&second_key], H256::repeat_byte(2));

        // Add more logs and check log ordering.
        let third_log = StorageLog::new_write_log(first_key, H256::repeat_byte(3));
        let more_logs = [(H256::repeat_byte(1), vec![third_log])];
        conn.storage_logs_dal()
            .append_storage_logs(MiniblockNumber(1), &more_logs)
            .await;
        conn.storage_dal().apply_storage_logs(&more_logs).await;

        let touched_slots = conn
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(L1BatchNumber(1))
            .await;
        assert_eq!(touched_slots.len(), 2);
        assert_eq!(touched_slots[&first_key], H256::repeat_byte(3));
        assert_eq!(touched_slots[&second_key], H256::repeat_byte(2));

        test_rollback(&mut conn, first_key, second_key).await;
    }

    async fn test_rollback(
        conn: &mut StorageProcessor<'_>,
        key: StorageKey,
        second_key: StorageKey,
    ) {
        let new_account = AccountTreeId::new(Address::repeat_byte(2));
        let new_key = StorageKey::new(new_account, H256::zero());
        let log = StorageLog::new_write_log(key, H256::repeat_byte(0xff));
        let other_log = StorageLog::new_write_log(second_key, H256::zero());
        let new_key_log = StorageLog::new_write_log(new_key, H256::repeat_byte(0xfe));
        let logs = vec![log, other_log, new_key_log];
        insert_miniblock(conn, 2, logs).await;

        let value = conn.storage_dal().get_by_key(&key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(0xff));
        let value = conn.storage_dal().get_by_key(&second_key).await.unwrap();
        assert_eq!(value, H256::zero());
        let value = conn.storage_dal().get_by_key(&new_key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(0xfe));

        let prev_keys = vec![key.hashed_key(), new_key.hashed_key(), H256::zero()];
        let prev_values = conn
            .storage_logs_dal()
            .get_previous_storage_values(&prev_keys, L1BatchNumber(2))
            .await;
        assert_eq!(prev_values.len(), 3);
        assert_eq!(prev_values[&prev_keys[0]], Some(H256::repeat_byte(3)));
        assert_eq!(prev_values[&prev_keys[1]], None);
        assert_eq!(prev_values[&prev_keys[2]], None);

        conn.storage_logs_dal()
            .rollback_storage(MiniblockNumber(1))
            .await;

        let value = conn.storage_dal().get_by_key(&key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(3));
        let value = conn.storage_dal().get_by_key(&second_key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(2));
        let value = conn.storage_dal().get_by_key(&new_key).await;
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn getting_storage_logs_for_revert() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();

        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.blocks_dal()
            .delete_l1_batches(L1BatchNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let logs: Vec<_> = (0_u8..10)
            .map(|i| {
                let key = StorageKey::new(account, H256::from_low_u64_be(u64::from(i)));
                StorageLog::new_write_log(key, H256::repeat_byte(i))
            })
            .collect();
        insert_miniblock(&mut conn, 1, logs.clone()).await;
        let written_keys: Vec<_> = logs.iter().map(|log| log.key).collect();
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(1), &written_keys)
            .await;

        let new_logs: Vec<_> = (5_u64..20)
            .map(|i| {
                let key = StorageKey::new(account, H256::from_low_u64_be(i));
                StorageLog::new_write_log(key, H256::from_low_u64_be(i))
            })
            .collect();
        insert_miniblock(&mut conn, 2, new_logs.clone()).await;
        let new_written_keys: Vec<_> = new_logs[5..].iter().map(|log| log.key).collect();
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(2), &new_written_keys)
            .await;

        let logs_for_revert = conn
            .storage_logs_dal()
            .get_storage_logs_for_revert(L1BatchNumber(1))
            .await;
        assert_eq!(logs_for_revert.len(), 15); // 5 updated + 10 new keys
        for log in &logs[5..] {
            let prev_value = logs_for_revert[&log.key.hashed_key()].unwrap().0;
            assert_eq!(prev_value, log.value);
        }
        for log in &new_logs[5..] {
            assert!(logs_for_revert[&log.key.hashed_key()].is_none());
        }
    }

    #[tokio::test]
    async fn reverting_keys_without_initial_write() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();

        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.blocks_dal()
            .delete_l1_batches(L1BatchNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let mut logs: Vec<_> = [0_u8, 1, 2, 3]
            .iter()
            .map(|&i| {
                let key = StorageKey::new(account, H256::from_low_u64_be(u64::from(i)));
                StorageLog::new_write_log(key, H256::repeat_byte(i % 3))
            })
            .collect();

        for l1_batch in [1, 2] {
            if l1_batch == 2 {
                for log in &mut logs[1..] {
                    log.value = H256::repeat_byte(0xff);
                }
            }
            insert_miniblock(&mut conn, l1_batch, logs.clone()).await;

            let all_keys: Vec<_> = logs.iter().map(|log| log.key.hashed_key()).collect();
            let non_initial = conn
                .storage_logs_dedup_dal()
                .filter_written_slots(&all_keys)
                .await;
            // Pretend that dedup logic eliminates all writes with zero values.
            let initial_keys: Vec<_> = logs
                .iter()
                .filter_map(|log| {
                    (!log.value.is_zero() && !non_initial.contains(&log.key.hashed_key()))
                        .then_some(log.key)
                })
                .collect();

            assert!(initial_keys.len() < logs.len());
            conn.storage_logs_dedup_dal()
                .insert_initial_writes(L1BatchNumber(l1_batch), &initial_keys)
                .await;
        }

        let logs_for_revert = conn
            .storage_logs_dal()
            .get_storage_logs_for_revert(L1BatchNumber(1))
            .await;
        assert_eq!(logs_for_revert.len(), 3);
        for (i, log) in logs.iter().enumerate() {
            let hashed_key = log.key.hashed_key();
            match i {
                // Key is deduped.
                0 => assert!(!logs_for_revert.contains_key(&hashed_key)),
                // Key is present in both batches as per `storage_logs` and `initial_writes`
                1 | 2 => assert!(logs_for_revert[&hashed_key].is_some()),
                // Key is present in both batches as per `storage_logs`, but `initial_writes`
                // indicates that the first write was deduped.
                3 => assert!(logs_for_revert[&hashed_key].is_none()),
                _ => unreachable!("we only have 4 keys"),
            }
        }
    }
}
