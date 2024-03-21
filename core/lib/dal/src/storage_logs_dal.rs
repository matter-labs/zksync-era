use std::{collections::HashMap, ops, time::Instant};

use sqlx::types::chrono::Utc;
use zksync_db_connection::{
    connection::Connection, instrument::InstrumentExt, write_str, writeln_str,
};
use zksync_types::{
    get_code_key, snapshots::SnapshotStorageLog, AccountTreeId, Address, L1BatchNumber,
    MiniblockNumber, StorageKey, StorageLog, FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H160, H256,
};

pub use crate::models::storage_log::{DbStorageLog, StorageRecoveryLogEntry};
use crate::{Core, CoreDal};

#[derive(Debug)]
pub struct StorageLogsDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl StorageLogsDal<'_, '_> {
    /// Inserts storage logs grouped by transaction for a miniblock. The ordering of transactions
    /// must be the same as their ordering in the miniblock.
    pub async fn insert_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
    ) -> sqlx::Result<()> {
        self.insert_storage_logs_inner(block_number, logs, 0).await
    }

    async fn insert_storage_logs_inner(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
        mut operation_number: u32,
    ) -> sqlx::Result<()> {
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
            .await?;

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
        copy.send(buffer.as_bytes()).await?;
        copy.finish().await?;
        Ok(())
    }

    pub async fn insert_storage_logs_from_snapshot(
        &mut self,
        miniblock_number: MiniblockNumber,
        snapshot_storage_logs: &[SnapshotStorageLog],
    ) -> sqlx::Result<()> {
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
            .await?;

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
        for log in snapshot_storage_logs.iter() {
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
                r"{}|\\x{:x}|{miniblock_number}|{now}|{now}",
                log.enumeration_index,
                H256::zero()
            );
        }
        copy.send(buffer.as_bytes()).await?;
        copy.finish().await?;
        Ok(())
    }

    pub async fn append_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
    ) -> sqlx::Result<()> {
        let operation_number = sqlx::query!(
            r#"
            SELECT
                MAX(operation_number) AS "max?"
            FROM
                storage_logs
            WHERE
                miniblock_number = $1
            "#,
            i64::from(block_number.0)
        )
        .fetch_one(self.storage.conn())
        .await?
        .max
        .map(|max| max as u32 + 1)
        .unwrap_or(0);

        self.insert_storage_logs_inner(block_number, logs, operation_number)
            .await
    }

    /// Rolls back storage to the specified point in time.
    #[deprecated(note = "`storage` table is soft-removed")]
    pub async fn rollback_storage(
        &mut self,
        last_miniblock_to_keep: MiniblockNumber,
    ) -> sqlx::Result<()> {
        let stage_start = Instant::now();
        let modified_keys = self
            .modified_keys_in_miniblocks(last_miniblock_to_keep.next()..=MiniblockNumber(u32::MAX))
            .await?;
        tracing::info!(
            "Loaded {} keys changed after miniblock #{last_miniblock_to_keep} in {:?}",
            modified_keys.len(),
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        let prev_values = self
            .get_storage_values(&modified_keys, last_miniblock_to_keep)
            .await?;
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
            r#"
            DELETE FROM storage
            WHERE
                hashed_key = ANY ($1)
            "#,
            &keys_to_delete as &[&[u8]],
        )
        .execute(self.storage.conn())
        .await?;

        tracing::info!(
            "Removed {} keys in {:?}",
            keys_to_delete.len(),
            stage_start.elapsed()
        );

        let stage_start = Instant::now();
        sqlx::query!(
            r#"
            UPDATE storage
            SET
                value = u.value
            FROM
                UNNEST($1::bytea[], $2::bytea[]) AS u (key, value)
            WHERE
                u.key = hashed_key
            "#,
            &keys_to_update as &[&[u8]],
            &values_to_update as &[&[u8]],
        )
        .execute(self.storage.conn())
        .await?;

        tracing::info!(
            "Updated {} keys to previous values in {:?}",
            keys_to_update.len(),
            stage_start.elapsed()
        );
        Ok(())
    }

    /// Returns distinct hashed storage keys that were modified in the specified miniblock range.
    pub async fn modified_keys_in_miniblocks(
        &mut self,
        miniblock_numbers: ops::RangeInclusive<MiniblockNumber>,
    ) -> sqlx::Result<Vec<H256>> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT
                hashed_key
            FROM
                storage_logs
            WHERE
                miniblock_number BETWEEN $1 AND $2
            "#,
            i64::from(miniblock_numbers.start().0),
            i64::from(miniblock_numbers.end().0)
        )
        .fetch_all(self.storage.conn())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| H256::from_slice(&row.hashed_key))
            .collect())
    }

    /// Removes all storage logs with a miniblock number strictly greater than the specified `block_number`.
    pub async fn rollback_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM storage_logs
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    /// Loads (hashed_key, value, operation_number) tuples for given miniblock_number.
    /// Uses provided DB table.
    /// Shouldn't be used in production.
    pub async fn get_miniblock_storage_logs(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Vec<(H256, H256, u32)> {
        sqlx::query!(
            r#"
            SELECT
                hashed_key,
                value,
                operation_number
            FROM
                storage_logs
            WHERE
                miniblock_number = $1
            ORDER BY
                operation_number
            "#,
            i64::from(miniblock_number.0)
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            let hashed_key = H256::from_slice(&row.hashed_key);
            let value = H256::from_slice(&row.value);
            let operation_number: u32 = row.operation_number as u32;
            (hashed_key, value, operation_number)
        })
        .collect()
    }

    pub async fn is_contract_deployed_at_address(&mut self, address: Address) -> bool {
        let hashed_key = get_code_key(&address).hashed_key();
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                (
                    SELECT
                        *
                    FROM
                        storage_logs
                    WHERE
                        storage_logs.hashed_key = $1
                    ORDER BY
                        storage_logs.miniblock_number DESC,
                        storage_logs.operation_number DESC
                    LIMIT
                        1
                ) sl
            WHERE
                sl.value != $2
            "#,
            hashed_key.as_bytes(),
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        row.count > 0
    }

    /// Returns addresses and the corresponding deployment miniblock numbers among the specified contract
    /// `addresses`. `at_miniblock` allows filtering deployment by miniblocks.
    pub async fn filter_deployed_contracts(
        &mut self,
        addresses: impl Iterator<Item = Address>,
        at_miniblock: Option<MiniblockNumber>,
    ) -> sqlx::Result<HashMap<Address, MiniblockNumber>> {
        let (bytecode_hashed_keys, address_by_hashed_key): (Vec<_>, HashMap<_, _>) = addresses
            .map(|address| {
                let hashed_key = get_code_key(&address).hashed_key().0;
                (hashed_key, (hashed_key, address))
            })
            .unzip();
        let max_miniblock_number = at_miniblock.map_or(u32::MAX, |number| number.0);
        // Get the latest `value` and corresponding `miniblock_number` for each of `bytecode_hashed_keys`. For failed deployments,
        // this value will equal `FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH`, so that they can be easily filtered.
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT
                ON (hashed_key) hashed_key,
                miniblock_number,
                value
            FROM
                storage_logs
            WHERE
                hashed_key = ANY ($1)
                AND miniblock_number <= $2
            ORDER BY
                hashed_key,
                miniblock_number DESC,
                operation_number DESC
            "#,
            &bytecode_hashed_keys as &[_],
            i64::from(max_miniblock_number)
        )
        .fetch_all(self.storage.conn())
        .await?;

        let deployment_data = rows.into_iter().filter_map(|row| {
            if row.value == FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes() {
                return None;
            }
            let miniblock_number = MiniblockNumber(row.miniblock_number as u32);
            let address = address_by_hashed_key[row.hashed_key.as_slice()];
            Some((address, miniblock_number))
        });
        Ok(deployment_data.collect())
    }

    /// Returns latest values for all [`StorageKey`]s written to in the specified L1 batch
    /// judging by storage logs (i.e., not taking deduplication logic into account).
    pub async fn get_touched_slots_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<HashMap<StorageKey, H256>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                address,
                key,
                value
            FROM
                storage_logs
            WHERE
                miniblock_number BETWEEN (
                    SELECT
                        MIN(number)
                    FROM
                        miniblocks
                    WHERE
                        l1_batch_number = $1
                ) AND (
                    SELECT
                        MAX(number)
                    FROM
                        miniblocks
                    WHERE
                        l1_batch_number = $1
                )
            ORDER BY
                miniblock_number,
                operation_number
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?;

        let touched_slots = rows.into_iter().map(|row| {
            let key = StorageKey::new(
                AccountTreeId::new(Address::from_slice(&row.address)),
                H256::from_slice(&row.key),
            );
            (key, H256::from_slice(&row.value))
        });
        Ok(touched_slots.collect())
    }

    /// Returns (hashed) storage keys and the corresponding values that need to be applied to a storage
    /// in order to revert it to the specified L1 batch. Deduplication is taken into account.
    pub async fn get_storage_logs_for_revert(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<HashMap<H256, Option<(H256, u64)>>> {
        let miniblock_range = self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await?;
        let Some((_, last_miniblock)) = miniblock_range else {
            return Ok(HashMap::new());
        };

        let stage_start = Instant::now();
        let mut modified_keys = self
            .modified_keys_in_miniblocks(last_miniblock.next()..=MiniblockNumber(u32::MAX))
            .await?;
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
            .await?;
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
            .await?
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
        Ok(output)
    }

    pub async fn get_l1_batches_and_indices_for_initial_writes(
        &mut self,
        hashed_keys: &[H256],
    ) -> sqlx::Result<HashMap<H256, (L1BatchNumber, u64)>> {
        if hashed_keys.is_empty() {
            return Ok(HashMap::new()); // Shortcut to save time on communication with DB in the common case
        }

        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();
        let rows = sqlx::query!(
            r#"
            SELECT
                hashed_key,
                l1_batch_number,
                INDEX
            FROM
                initial_writes
            WHERE
                hashed_key = ANY ($1::bytea[])
            "#,
            &hashed_keys as &[&[u8]],
        )
        .instrument("get_l1_batches_and_indices_for_initial_writes")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    H256::from_slice(&row.hashed_key),
                    (L1BatchNumber(row.l1_batch_number as u32), row.index as u64),
                )
            })
            .collect())
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
    ) -> sqlx::Result<HashMap<H256, Option<H256>>> {
        let (miniblock_number, _) = self
            .storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(next_l1_batch)
            .await?
            .unwrap();

        if miniblock_number == MiniblockNumber(0) {
            Ok(hashed_keys.iter().copied().map(|key| (key, None)).collect())
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
    ) -> sqlx::Result<HashMap<H256, Option<H256>>> {
        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();

        let rows = sqlx::query!(
            r#"
            SELECT
                u.hashed_key AS "hashed_key!",
                (
                    SELECT
                        value
                    FROM
                        storage_logs
                    WHERE
                        hashed_key = u.hashed_key
                        AND miniblock_number <= $2
                    ORDER BY
                        miniblock_number DESC,
                        operation_number DESC
                    LIMIT
                        1
                ) AS "value?"
            FROM
                UNNEST($1::bytea[]) AS u (hashed_key)
            "#,
            &hashed_keys as &[&[u8]],
            i64::from(miniblock_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let key = H256::from_slice(&row.hashed_key);
                let value = row.value.map(|value| H256::from_slice(&value));
                (key, value)
            })
            .collect())
    }

    /// Retrieves all storage log entries for testing purposes.
    pub async fn dump_all_storage_logs_for_tests(&mut self) -> Vec<DbStorageLog> {
        let rows = sqlx::query!(
            r#"
            SELECT
                hashed_key,
                address,
                key,
                value,
                operation_number,
                tx_hash,
                miniblock_number
            FROM
                storage_logs
            "#
        )
        .fetch_all(self.storage.conn())
        .await
        .expect("get_all_storage_logs_for_tests");

        rows.into_iter()
            .map(|row| DbStorageLog {
                hashed_key: H256::from_slice(&row.hashed_key),
                address: H160::from_slice(&row.address),
                key: H256::from_slice(&row.key),
                value: H256::from_slice(&row.value),
                operation_number: row.operation_number as u64,
                tx_hash: H256::from_slice(&row.tx_hash),
                miniblock_number: MiniblockNumber(row.miniblock_number as u32),
            })
            .collect()
    }

    /// Returns the total number of rows in the `storage_logs` table before and at the specified miniblock.
    ///
    /// **Warning.** This method is slow (requires a full table scan).
    pub async fn get_storage_logs_row_count(
        &mut self,
        at_miniblock: MiniblockNumber,
    ) -> sqlx::Result<u64> {
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS COUNT
            FROM
                storage_logs
            WHERE
                miniblock_number <= $1
            "#,
            i64::from(at_miniblock.0)
        )
        .instrument("get_storage_logs_row_count")
        .with_arg("miniblock_number", &at_miniblock)
        .report_latency()
        .expect_slow_query()
        .fetch_one(self.storage)
        .await?;
        Ok(row.count.unwrap_or(0) as u64)
    }

    /// Gets a starting tree entry for each of the supplied `key_ranges` for the specified
    /// `miniblock_number`. This method is used during Merkle tree recovery.
    pub async fn get_chunk_starts_for_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
        key_ranges: &[ops::RangeInclusive<H256>],
    ) -> sqlx::Result<Vec<Option<StorageRecoveryLogEntry>>> {
        let (start_keys, end_keys): (Vec<_>, Vec<_>) = key_ranges
            .iter()
            .map(|range| (range.start().as_bytes(), range.end().as_bytes()))
            .unzip();
        let rows = sqlx::query!(
            r#"
            WITH
                sl AS (
                    SELECT
                        (
                            SELECT
                                ARRAY[hashed_key, value] AS kv
                            FROM
                                storage_logs
                            WHERE
                                storage_logs.miniblock_number = $1
                                AND storage_logs.hashed_key >= u.start_key
                                AND storage_logs.hashed_key <= u.end_key
                            ORDER BY
                                storage_logs.hashed_key
                            LIMIT
                                1
                        )
                    FROM
                        UNNEST($2::bytea[], $3::bytea[]) AS u (start_key, end_key)
                )
            SELECT
                sl.kv[1] AS "hashed_key?",
                sl.kv[2] AS "value?",
                initial_writes.index
            FROM
                sl
                LEFT OUTER JOIN initial_writes ON initial_writes.hashed_key = sl.kv[1]
            "#,
            i64::from(miniblock_number.0),
            &start_keys as &[&[u8]],
            &end_keys as &[&[u8]],
        )
        .fetch_all(self.storage.conn())
        .await?;

        let rows = rows.into_iter().map(|row| {
            Some(StorageRecoveryLogEntry {
                key: H256::from_slice(row.hashed_key.as_ref()?),
                value: H256::from_slice(row.value.as_ref()?),
                leaf_index: row.index? as u64,
            })
        });
        Ok(rows.collect())
    }

    /// Fetches tree entries for the specified `miniblock_number` and `key_range`. This is used during
    /// Merkle tree recovery.
    pub async fn get_tree_entries_for_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
        key_range: ops::RangeInclusive<H256>,
    ) -> sqlx::Result<Vec<StorageRecoveryLogEntry>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                storage_logs.hashed_key,
                storage_logs.value,
                initial_writes.index
            FROM
                storage_logs
                INNER JOIN initial_writes ON storage_logs.hashed_key = initial_writes.hashed_key
            WHERE
                storage_logs.miniblock_number = $1
                AND storage_logs.hashed_key >= $2::bytea
                AND storage_logs.hashed_key <= $3::bytea
            ORDER BY
                storage_logs.hashed_key
            "#,
            i64::from(miniblock_number.0),
            key_range.start().as_bytes(),
            key_range.end().as_bytes()
        )
        .fetch_all(self.storage.conn())
        .await?;

        let rows = rows.into_iter().map(|row| StorageRecoveryLogEntry {
            key: H256::from_slice(&row.hashed_key),
            value: H256::from_slice(&row.value),
            leaf_index: row.index as u64,
        });
        Ok(rows.collect())
    }
}

#[cfg(test)]
mod tests {
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{block::L1BatchHeader, ProtocolVersion, ProtocolVersionId};

    use super::*;
    use crate::{tests::create_miniblock_header, ConnectionPool, Core};

    async fn insert_miniblock(conn: &mut Connection<'_, Core>, number: u32, logs: Vec<StorageLog>) {
        let header = L1BatchHeader::new(
            L1BatchNumber(number),
            0,
            BaseSystemContractsHashes::default(),
            ProtocolVersionId::default(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(number))
            .await
            .unwrap();

        let logs = [(H256::zero(), logs)];
        conn.storage_logs_dal()
            .insert_storage_logs(MiniblockNumber(number), &logs)
            .await
            .unwrap();
        #[allow(deprecated)]
        conn.storage_dal().apply_storage_logs(&logs).await;
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(number))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn inserting_storage_logs() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
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
            .await
            .unwrap();
        assert_eq!(touched_slots.len(), 2);
        assert_eq!(touched_slots[&first_key], H256::repeat_byte(1));
        assert_eq!(touched_slots[&second_key], H256::repeat_byte(2));

        // Add more logs and check log ordering.
        let third_log = StorageLog::new_write_log(first_key, H256::repeat_byte(3));
        let more_logs = [(H256::repeat_byte(1), vec![third_log])];
        conn.storage_logs_dal()
            .append_storage_logs(MiniblockNumber(1), &more_logs)
            .await
            .unwrap();
        #[allow(deprecated)]
        conn.storage_dal().apply_storage_logs(&more_logs).await;

        let touched_slots = conn
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();
        assert_eq!(touched_slots.len(), 2);
        assert_eq!(touched_slots[&first_key], H256::repeat_byte(3));
        assert_eq!(touched_slots[&second_key], H256::repeat_byte(2));

        test_rollback(&mut conn, first_key, second_key).await;
    }

    async fn test_rollback(
        conn: &mut Connection<'_, Core>,
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

        let value = conn.storage_web3_dal().get_value(&key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(0xff));
        let value = conn
            .storage_web3_dal()
            .get_value(&second_key)
            .await
            .unwrap();
        assert_eq!(value, H256::zero());
        let value = conn.storage_web3_dal().get_value(&new_key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(0xfe));

        // Check the outdated `storage` table as well.
        #[allow(deprecated)]
        {
            let value = conn.storage_dal().get_by_key(&key).await.unwrap();
            assert_eq!(value, Some(H256::repeat_byte(0xff)));
            let value = conn.storage_dal().get_by_key(&second_key).await.unwrap();
            assert_eq!(value, Some(H256::zero()));
            let value = conn.storage_dal().get_by_key(&new_key).await.unwrap();
            assert_eq!(value, Some(H256::repeat_byte(0xfe)));
        }

        let prev_keys = vec![key.hashed_key(), new_key.hashed_key(), H256::zero()];
        let prev_values = conn
            .storage_logs_dal()
            .get_previous_storage_values(&prev_keys, L1BatchNumber(2))
            .await
            .unwrap();
        assert_eq!(prev_values.len(), 3);
        assert_eq!(prev_values[&prev_keys[0]], Some(H256::repeat_byte(3)));
        assert_eq!(prev_values[&prev_keys[1]], None);
        assert_eq!(prev_values[&prev_keys[2]], None);

        #[allow(deprecated)]
        {
            conn.storage_logs_dal()
                .rollback_storage(MiniblockNumber(1))
                .await
                .unwrap();
            let value = conn.storage_dal().get_by_key(&key).await.unwrap();
            assert_eq!(value, Some(H256::repeat_byte(3)));
            let value = conn.storage_dal().get_by_key(&second_key).await.unwrap();
            assert_eq!(value, Some(H256::repeat_byte(2)));
            let value = conn.storage_dal().get_by_key(&new_key).await.unwrap();
            assert_eq!(value, None);
        }

        conn.storage_logs_dal()
            .rollback_storage_logs(MiniblockNumber(1))
            .await
            .unwrap();

        let value = conn.storage_web3_dal().get_value(&key).await.unwrap();
        assert_eq!(value, H256::repeat_byte(3));
        let value = conn
            .storage_web3_dal()
            .get_value(&second_key)
            .await
            .unwrap();
        assert_eq!(value, H256::repeat_byte(2));
        let value = conn.storage_web3_dal().get_value(&new_key).await.unwrap();
        assert_eq!(value, H256::zero());
    }

    #[tokio::test]
    async fn getting_storage_logs_for_revert() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
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
            .await
            .unwrap();

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
            .await
            .unwrap();

        let logs_for_revert = conn
            .storage_logs_dal()
            .get_storage_logs_for_revert(L1BatchNumber(1))
            .await
            .unwrap();
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
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
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
                .await
                .unwrap();
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
                .await
                .unwrap();
        }

        let logs_for_revert = conn
            .storage_logs_dal()
            .get_storage_logs_for_revert(L1BatchNumber(1))
            .await
            .unwrap();
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

    #[tokio::test]
    async fn getting_starting_entries_in_chunks() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let sorted_hashed_keys = prepare_tree_entries(&mut conn, 100).await;

        let key_ranges = [
            H256::zero()..=H256::repeat_byte(0xff),
            H256::repeat_byte(0x40)..=H256::repeat_byte(0x80),
            H256::repeat_byte(0x50)..=H256::repeat_byte(0x60),
            H256::repeat_byte(0x50)..=H256::repeat_byte(0x51),
            H256::repeat_byte(0xb0)..=H256::repeat_byte(0xfe),
            H256::repeat_byte(0x11)..=H256::repeat_byte(0x11),
        ];

        let chunk_starts = conn
            .storage_logs_dal()
            .get_chunk_starts_for_miniblock(MiniblockNumber(1), &key_ranges)
            .await
            .unwrap();

        for (chunk_start, key_range) in chunk_starts.into_iter().zip(key_ranges) {
            let expected_start_key = sorted_hashed_keys
                .iter()
                .find(|&key| key_range.contains(key));
            if let Some(chunk_start) = chunk_start {
                assert_eq!(chunk_start.key, *expected_start_key.unwrap());
                assert_ne!(chunk_start.value, H256::zero());
                assert_ne!(chunk_start.leaf_index, 0);
            } else {
                assert_eq!(expected_start_key, None);
            }
        }
    }

    async fn prepare_tree_entries(conn: &mut Connection<'_, Core>, count: u8) -> Vec<H256> {
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let logs: Vec<_> = (0..count)
            .map(|i| {
                let key = StorageKey::new(account, H256::repeat_byte(i));
                StorageLog::new_write_log(key, H256::repeat_byte(i))
            })
            .collect();
        insert_miniblock(conn, 1, logs.clone()).await;

        let mut initial_keys: Vec<_> = logs.iter().map(|log| log.key).collect();
        initial_keys.sort_unstable();
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(1), &initial_keys)
            .await
            .unwrap();

        let mut sorted_hashed_keys: Vec<_> = logs.iter().map(|log| log.key.hashed_key()).collect();
        sorted_hashed_keys.sort_unstable();
        sorted_hashed_keys
    }

    #[tokio::test]
    async fn getting_tree_entries() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let sorted_hashed_keys = prepare_tree_entries(&mut conn, 10).await;

        let key_range = H256::zero()..=H256::repeat_byte(0xff);
        let tree_entries = conn
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(MiniblockNumber(1), key_range)
            .await
            .unwrap();
        assert_eq!(tree_entries.len(), 10);
        assert_eq!(
            tree_entries
                .iter()
                .map(|entry| entry.key)
                .collect::<Vec<_>>(),
            sorted_hashed_keys
        );

        let key_range = H256::repeat_byte(0x80)..=H256::repeat_byte(0xbf);
        let tree_entries = conn
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(MiniblockNumber(1), key_range.clone())
            .await
            .unwrap();
        assert!(!tree_entries.is_empty() && tree_entries.len() < 10);
        for entry in &tree_entries {
            assert!(key_range.contains(&entry.key));
        }
    }

    #[tokio::test]
    async fn filtering_deployed_contracts() {
        let contract_address = Address::repeat_byte(1);
        let other_contract_address = Address::repeat_byte(23);
        let successful_deployment =
            StorageLog::new_write_log(get_code_key(&contract_address), H256::repeat_byte(0xff));
        let failed_deployment = StorageLog::new_write_log(
            get_code_key(&contract_address),
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
        );

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        // If deployment fails then two writes are issued, one that writes `bytecode_hash` to the "correct" value,
        // and the next write reverts its value back to `FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH`.
        conn.storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(1),
                &[(H256::zero(), vec![successful_deployment, failed_deployment])],
            )
            .await
            .unwrap();

        let tested_miniblocks = [
            None,
            Some(MiniblockNumber(0)),
            Some(MiniblockNumber(1)),
            Some(MiniblockNumber(1)),
        ];
        for at_miniblock in tested_miniblocks {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    at_miniblock,
                )
                .await
                .unwrap();
            assert!(
                deployed_map.is_empty(),
                "{deployed_map:?} at miniblock {at_miniblock:?}"
            );
        }

        conn.storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(2),
                &[(H256::zero(), vec![successful_deployment])],
            )
            .await
            .unwrap();

        for old_miniblock in [MiniblockNumber(0), MiniblockNumber(1)] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    Some(old_miniblock),
                )
                .await
                .unwrap();
            assert!(
                deployed_map.is_empty(),
                "{deployed_map:?} at {old_miniblock}"
            );
        }
        for new_miniblock in [None, Some(MiniblockNumber(2))] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    new_miniblock,
                )
                .await
                .unwrap();
            assert_eq!(
                deployed_map,
                HashMap::from([(contract_address, MiniblockNumber(2))])
            );
        }

        let other_successful_deployment = StorageLog::new_write_log(
            get_code_key(&other_contract_address),
            H256::repeat_byte(0xff),
        );
        conn.storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(3),
                &[(H256::zero(), vec![other_successful_deployment])],
            )
            .await
            .unwrap();

        for old_miniblock in [MiniblockNumber(0), MiniblockNumber(1)] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    Some(old_miniblock),
                )
                .await
                .unwrap();
            assert!(
                deployed_map.is_empty(),
                "{deployed_map:?} at miniblock {old_miniblock}"
            );
        }

        let deployed_map = conn
            .storage_logs_dal()
            .filter_deployed_contracts(
                [contract_address, other_contract_address].into_iter(),
                Some(MiniblockNumber(2)),
            )
            .await
            .unwrap();
        assert_eq!(
            deployed_map,
            HashMap::from([(contract_address, MiniblockNumber(2))])
        );

        for new_miniblock in [None, Some(MiniblockNumber(3))] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    new_miniblock,
                )
                .await
                .unwrap();
            assert_eq!(
                deployed_map,
                HashMap::from([
                    (contract_address, MiniblockNumber(2)),
                    (other_contract_address, MiniblockNumber(3)),
                ])
            );
        }
    }
}
