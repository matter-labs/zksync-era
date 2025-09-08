use std::{collections::HashMap, num::NonZeroU32, ops, time::Instant};

use sqlx::types::chrono::Utc;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{CopyStatement, InstrumentExt},
    write_str, writeln_str,
};
use zksync_types::{
    get_code_key, h256_to_u256, snapshots::SnapshotStorageLog, u256_to_h256, AccountTreeId,
    Address, L1BatchNumber, L2BlockNumber, StorageKey, StorageLog,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H160, H256,
};

pub use crate::models::storage_log::{DbStorageLog, StorageRecoveryLogEntry};
use crate::{Core, CoreDal};

#[derive(Debug)]
pub struct StorageLogsDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl StorageLogsDal<'_, '_> {
    /// Inserts storage logs grouped by transaction for an L2 block. The ordering of transactions
    /// must be the same as their ordering in the L2 block.
    pub async fn insert_storage_logs(
        &mut self,
        block_number: L2BlockNumber,
        logs: &[StorageLog],
    ) -> DalResult<()> {
        self.insert_storage_logs_inner(block_number, logs, 0).await
    }

    async fn insert_storage_logs_inner(
        &mut self,
        block_number: L2BlockNumber,
        logs: &[StorageLog],
        mut operation_number: u32,
    ) -> DalResult<()> {
        let logs_len = logs.len();
        let copy = CopyStatement::new(
            "COPY storage_logs(
                hashed_key, address, key, value, operation_number, miniblock_number,
                created_at, updated_at
            )
            FROM STDIN WITH (DELIMITER '|')",
        )
        .instrument("insert_storage_logs")
        .with_arg("block_number", &block_number)
        .with_arg("logs.len", &logs_len)
        .start(self.storage)
        .await?;

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
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
                r"{operation_number}|{block_number}|{now}|{now}"
            );

            operation_number += 1;
        }
        copy.send(buffer.as_bytes()).await
    }

    #[deprecated(note = "Will be removed in favor of `insert_storage_logs_from_snapshot()`")]
    pub async fn insert_storage_logs_with_preimages_from_snapshot(
        &mut self,
        l2_block_number: L2BlockNumber,
        snapshot_storage_logs: &[SnapshotStorageLog<StorageKey>],
    ) -> DalResult<()> {
        let storage_logs_len = snapshot_storage_logs.len();
        let copy = CopyStatement::new(
            "COPY storage_logs(
                hashed_key, address, key, value, operation_number, tx_hash, miniblock_number,
                created_at, updated_at
            )
            FROM STDIN WITH (DELIMITER '|')",
        )
        .instrument("insert_storage_logs_from_snapshot")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("storage_logs.len", &storage_logs_len)
        .start(self.storage)
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
                r"{}|\\x{:x}|{l2_block_number}|{now}|{now}",
                log.enumeration_index,
                H256::zero()
            );
        }
        copy.send(buffer.as_bytes()).await
    }

    pub async fn insert_storage_logs_from_snapshot(
        &mut self,
        l2_block_number: L2BlockNumber,
        snapshot_storage_logs: &[SnapshotStorageLog],
    ) -> DalResult<()> {
        let storage_logs_len = snapshot_storage_logs.len();
        let copy = CopyStatement::new(
            "COPY storage_logs(
                hashed_key, value, operation_number, tx_hash, miniblock_number,
                created_at, updated_at
            )
            FROM STDIN WITH (DELIMITER '|')",
        )
        .instrument("insert_storage_logs_from_snapshot")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("storage_logs.len", &storage_logs_len)
        .start(self.storage)
        .await?;

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
        for log in snapshot_storage_logs.iter() {
            write_str!(
                &mut buffer,
                r"\\x{hashed_key:x}|\\x{value:x}|",
                hashed_key = log.key,
                value = log.value
            );
            writeln_str!(
                &mut buffer,
                r"{}|\\x{:x}|{l2_block_number}|{now}|{now}",
                log.enumeration_index,
                H256::zero()
            );
        }
        copy.send(buffer.as_bytes()).await
    }

    pub async fn append_storage_logs(
        &mut self,
        block_number: L2BlockNumber,
        logs: &[StorageLog],
    ) -> DalResult<()> {
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
        .instrument("append_storage_logs#get_operation_number")
        .with_arg("block_number", &block_number)
        .fetch_one(self.storage)
        .await?
        .max
        .map(|max| max as u32 + 1)
        .unwrap_or(0);

        self.insert_storage_logs_inner(block_number, logs, operation_number)
            .await
    }

    /// Returns distinct hashed storage keys that were modified in the specified L2 block range.
    pub async fn modified_keys_in_l2_blocks(
        &mut self,
        l2_block_numbers: ops::RangeInclusive<L2BlockNumber>,
    ) -> DalResult<Vec<H256>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                DISTINCT
                hashed_key
            FROM
                storage_logs
            WHERE
                miniblock_number BETWEEN $1 AND $2
            "#,
            i64::from(l2_block_numbers.start().0),
            i64::from(l2_block_numbers.end().0)
        )
        .instrument("modified_keys_in_l2_blocks")
        .with_arg("l2_block_numbers", &l2_block_numbers)
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| H256::from_slice(&row.hashed_key))
            .collect())
    }

    /// Removes all storage logs with a L2 block number strictly greater than the specified `block_number`.
    pub async fn roll_back_storage_logs(&mut self, block_number: L2BlockNumber) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM storage_logs
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .instrument("roll_back_storage_logs")
        .with_arg("block_number", &block_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Returns addresses and the corresponding deployment L2 block numbers among the specified contract
    /// `addresses`. `at_l2_block` allows filtering deployment by L2 blocks.
    pub async fn filter_deployed_contracts(
        &mut self,
        addresses: impl Iterator<Item = Address>,
        at_l2_block: Option<L2BlockNumber>,
    ) -> DalResult<HashMap<Address, (L2BlockNumber, H256)>> {
        let (bytecode_hashed_keys, address_by_hashed_key): (Vec<_>, HashMap<_, _>) = addresses
            .map(|address| {
                let hashed_key = get_code_key(&address).hashed_key().0;
                (hashed_key, (hashed_key, address))
            })
            .unzip();
        let max_l2_block_number = at_l2_block.map_or(u32::MAX, |number| number.0);
        // Get the latest `value` and corresponding `miniblock_number` for each of `bytecode_hashed_keys`. For failed deployments,
        // this value will equal `FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH`, so that they can be easily filtered.
        let rows = sqlx::query!(
            r#"
            SELECT
                sl.hashed_key,
                lat.miniblock_number,
                lat.value
            FROM
                storage_logs sl
            JOIN LATERAL (
                SELECT
                    miniblock_number,
                    value
                FROM
                    storage_logs
                WHERE
                    hashed_key = sl.hashed_key
                    AND miniblock_number <= $2
                    AND miniblock_number <= COALESCE(
                        (
                            SELECT
                                MAX(number)
                            FROM
                                miniblocks
                        ),
                        (
                            SELECT
                                miniblock_number
                            FROM
                                snapshot_recovery
                        )
                    )
                ORDER BY
                    miniblock_number DESC,
                    operation_number DESC
                LIMIT 1
            ) lat
                ON TRUE
            WHERE
                hashed_key = ANY($1)
            "#,
            &bytecode_hashed_keys as &[_],
            i64::from(max_l2_block_number)
        )
        .instrument("filter_deployed_contracts")
        .with_arg("addresses.len", &bytecode_hashed_keys.len())
        .with_arg("at_l2_block", &at_l2_block)
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        let deployment_data = rows.into_iter().filter_map(|row| {
            let bytecode_hash = H256::from_slice(&row.value);
            if bytecode_hash == FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH {
                return None;
            }
            let l2_block_number = L2BlockNumber(row.miniblock_number as u32);
            let address = address_by_hashed_key[row.hashed_key.as_slice()];
            Some((address, (l2_block_number, bytecode_hash)))
        });
        Ok(deployment_data.collect())
    }

    /// Returns latest values for all slots written to in the specified L1 batch
    /// judging by storage logs (i.e., not taking deduplication logic into account).
    pub async fn get_touched_slots_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<HashMap<H256, H256>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                hashed_key,
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
        .instrument("get_touched_slots_for_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?;

        let touched_slots = rows.into_iter().map(|row| {
            (
                H256::from_slice(&row.hashed_key),
                H256::from_slice(&row.value),
            )
        });
        Ok(touched_slots.collect())
    }

    /// Same as [`Self::get_touched_slots_for_l1_batch()`], but loads key preimages instead of hashed keys.
    /// Correspondingly, this method is safe to call for locally executed L1 batches, for which key preimages
    /// are known; otherwise, it will error.
    pub async fn get_touched_slots_for_executed_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<HashMap<StorageKey, H256>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                address AS "address!",
                key AS "key!",
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
        .instrument("get_touched_slots_for_executed_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
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
    ) -> DalResult<HashMap<H256, Option<(H256, u64)>>> {
        let l2_block_range = self
            .storage
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await?;
        let Some((_, last_l2_block)) = l2_block_range else {
            return Ok(HashMap::new());
        };

        let stage_start = Instant::now();
        let mut modified_keys = self
            .modified_keys_in_l2_blocks(last_l2_block.next()..=L2BlockNumber(u32::MAX))
            .await?;
        let modified_keys_count = modified_keys.len();
        tracing::info!(
            "Fetched {modified_keys_count} keys changed after L2 block #{last_l2_block} in {:?}",
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
            .get_storage_values(&modified_keys, last_l2_block)
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
    ) -> DalResult<HashMap<H256, (L1BatchNumber, u64)>> {
        if hashed_keys.is_empty() {
            return Ok(HashMap::new()); // Shortcut to save time on communication with DB in the common case
        }

        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();
        let rows = sqlx::query!(
            r#"
            SELECT
                hashed_key,
                l1_batch_number,
                index
            FROM
                initial_writes
            WHERE
                hashed_key = ANY($1::bytea [])
            "#,
            &hashed_keys as &[&[u8]],
        )
        .instrument("get_l1_batches_and_indices_for_initial_writes")
        .with_arg("hashed_keys.len", &hashed_keys.len())
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
    ) -> DalResult<HashMap<H256, Option<H256>>> {
        let (l2_block_number, _) = self
            .storage
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(next_l1_batch)
            .await?
            .unwrap();

        if l2_block_number == L2BlockNumber(0) {
            Ok(hashed_keys.iter().copied().map(|key| (key, None)).collect())
        } else {
            self.get_storage_values(hashed_keys, l2_block_number - 1)
                .await
        }
    }

    /// Returns current values for the specified keys at the specified `l2_block_number`.
    pub async fn get_storage_values(
        &mut self,
        hashed_keys: &[H256],
        l2_block_number: L2BlockNumber,
    ) -> DalResult<HashMap<H256, Option<H256>>> {
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
                UNNEST($1::bytea []) AS u (hashed_key)
            "#,
            &hashed_keys as &[&[u8]],
            i64::from(l2_block_number.0)
        )
        .instrument("get_storage_values")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("hashed_keys.len", &hashed_keys.len())
        .fetch_all(self.storage)
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
                miniblock_number
            FROM
                storage_logs
            ORDER BY
                miniblock_number,
                operation_number
            "#
        )
        .fetch_all(self.storage.conn())
        .await
        .expect("get_all_storage_logs_for_tests");

        rows.into_iter()
            .map(|row| DbStorageLog {
                hashed_key: H256::from_slice(&row.hashed_key),
                address: row.address.as_deref().map(H160::from_slice),
                key: row.key.as_deref().map(H256::from_slice),
                value: H256::from_slice(&row.value),
                operation_number: row.operation_number as u64,
                l2_block_number: L2BlockNumber(row.miniblock_number as u32),
            })
            .collect()
    }

    /// Returns the total number of rows in the `storage_logs` table before and at the specified L2 block.
    ///
    /// **Warning.** This method is slow (requires a full table scan).
    pub async fn get_storage_logs_row_count(
        &mut self,
        at_l2_block: L2BlockNumber,
    ) -> DalResult<u64> {
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS COUNT
            FROM
                STORAGE_LOGS
            WHERE
                MINIBLOCK_NUMBER <= $1
            "#,
            i64::from(at_l2_block.0)
        )
        .instrument("get_storage_logs_row_count")
        .with_arg("at_l2_block", &at_l2_block)
        .report_latency()
        .expect_slow_query()
        .fetch_one(self.storage)
        .await?;
        Ok(row.count.unwrap_or(0) as u64)
    }

    /// Gets a starting tree entry for each of the supplied `key_ranges` for the specified
    /// `l2_block_number`. This method is used during Merkle tree recovery.
    pub async fn get_chunk_starts_for_l2_block(
        &mut self,
        l2_block_number: L2BlockNumber,
        key_ranges: &[ops::RangeInclusive<H256>],
    ) -> DalResult<Vec<Option<StorageRecoveryLogEntry>>> {
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
                            storage_logs.miniblock_number <= $1
                            AND storage_logs.hashed_key >= u.start_key
                            AND storage_logs.hashed_key <= u.end_key
                        ORDER BY
                            storage_logs.hashed_key
                        LIMIT
                            1
                    )
                FROM
                    UNNEST($2::bytea [], $3::bytea []) AS u (start_key, end_key)
            )
            
            SELECT
                sl.kv[1] AS "hashed_key?",
                sl.kv[2] AS "value?",
                initial_writes.index
            FROM
                sl
            LEFT OUTER JOIN initial_writes ON initial_writes.hashed_key = sl.kv[1]
            "#,
            i64::from(l2_block_number.0),
            &start_keys as &[&[u8]],
            &end_keys as &[&[u8]],
        )
        .instrument("get_chunk_starts_for_l2_block")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("key_ranges.len", &key_ranges.len())
        .fetch_all(self.storage)
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

    /// Fetches tree entries for the specified `l2_block_number` and `key_range`. This is used during
    /// Merkle tree and RocksDB cache recovery.
    pub async fn get_tree_entries_for_l2_block(
        &mut self,
        l2_block_number: L2BlockNumber,
        mut key_range: ops::RangeInclusive<H256>,
    ) -> DalResult<Vec<StorageRecoveryLogEntry>> {
        const QUERY_LIMIT: usize = 10_000;

        // Break fetching from the DB into smaller chunks to make DB load more uniform.
        let mut entries = vec![];
        loop {
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
                    storage_logs.miniblock_number <= $1
                    AND storage_logs.hashed_key >= $2::bytea
                    AND storage_logs.hashed_key <= $3::bytea
                ORDER BY
                    storage_logs.hashed_key
                LIMIT
                    $4
                "#,
                i64::from(l2_block_number.0),
                key_range.start().as_bytes(),
                key_range.end().as_bytes(),
                QUERY_LIMIT as i32
            )
            .instrument("get_tree_entries_for_l2_block")
            .with_arg("l2_block_number", &l2_block_number)
            .with_arg("key_range", &key_range)
            .fetch_all(self.storage)
            .await?;

            let fetched_count = rows.len();
            entries.extend(rows.into_iter().map(|row| StorageRecoveryLogEntry {
                key: H256::from_slice(&row.hashed_key),
                value: H256::from_slice(&row.value),
                leaf_index: row.index as u64,
            }));

            if fetched_count < QUERY_LIMIT {
                break;
            }
            // `unwrap()` is safe: `entries` contains >= QUERY_LIMIT items.
            let Some(next_key) = h256_to_u256(entries.last().unwrap().key).checked_add(1.into())
            else {
                // A marginal case (likely not reproducible in practice): the last hashed key is `H256::repeat_byte(0xff)`.
                break;
            };
            key_range = u256_to_h256(next_key)..=*key_range.end();
        }

        Ok(entries)
    }

    /// Returns `true` if the number of logs at the specified L2 block is greater or equal to `min_count`.
    pub async fn check_storage_log_count(
        &mut self,
        l2_block_number: L2BlockNumber,
        min_count: NonZeroU32,
    ) -> DalResult<bool> {
        let offset = min_count.get() - 1; // Cannot underflow

        let row = sqlx::query_scalar!(
            r#"
                SELECT TRUE
                FROM storage_logs
                WHERE miniblock_number <= $1
                LIMIT 1 OFFSET $2
            "#,
            i64::from(l2_block_number.0),
            i64::from(offset)
        )
        .instrument("check_storage_log_count")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("offset", &offset)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.is_some())
    }
}

#[cfg(test)]
mod tests {
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::L1BatchHeader, settlement::SettlementLayer, AccountTreeId, ProtocolVersion,
        ProtocolVersionId, StorageKey,
    };

    use super::*;
    use crate::{tests::create_l2_block_header, ConnectionPool, Core};

    async fn insert_l2_block(conn: &mut Connection<'_, Core>, number: u32, logs: Vec<StorageLog>) {
        let header = L1BatchHeader::new(
            L1BatchNumber(number),
            0,
            BaseSystemContractsHashes::default(),
            ProtocolVersionId::default(),
            SettlementLayer::for_tests(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(number))
            .await
            .unwrap();

        conn.storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(number), &logs)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(number))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn inserting_storage_logs() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let first_key = StorageKey::new(account, H256::zero());
        let second_key = StorageKey::new(account, H256::from_low_u64_be(1));
        let log = StorageLog::new_write_log(first_key, H256::repeat_byte(1));
        let other_log = StorageLog::new_write_log(second_key, H256::repeat_byte(2));
        insert_l2_block(&mut conn, 1, vec![log, other_log]).await;

        // Check for `L2BlockNumber(0)` at which no logs are inserted.
        for count in [1, 2, 3, 4, 10, 100_000, u32::MAX] {
            println!("count = {count}");
            assert!(!conn
                .storage_logs_dal()
                .check_storage_log_count(L2BlockNumber(0), NonZeroU32::new(count).unwrap())
                .await
                .unwrap());
        }

        for satisfying_count in [1, 2] {
            println!("count = {satisfying_count}");
            assert!(conn
                .storage_logs_dal()
                .check_storage_log_count(
                    L2BlockNumber(1),
                    NonZeroU32::new(satisfying_count).unwrap()
                )
                .await
                .unwrap());
        }
        for larger_count in [3, 4, 10, 100_000, u32::MAX] {
            println!("count = {larger_count}");
            assert!(!conn
                .storage_logs_dal()
                .check_storage_log_count(L2BlockNumber(1), NonZeroU32::new(larger_count).unwrap())
                .await
                .unwrap());
        }

        let touched_slots = conn
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();
        assert_eq!(touched_slots.len(), 2);
        assert_eq!(touched_slots[&first_key.hashed_key()], H256::repeat_byte(1));
        assert_eq!(
            touched_slots[&second_key.hashed_key()],
            H256::repeat_byte(2)
        );

        // Add more logs and check log ordering.
        let third_log = StorageLog::new_write_log(first_key, H256::repeat_byte(3));
        let more_logs = vec![third_log];
        conn.storage_logs_dal()
            .append_storage_logs(L2BlockNumber(1), &more_logs)
            .await
            .unwrap();

        let touched_slots = conn
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();
        assert_eq!(touched_slots.len(), 2);
        assert_eq!(touched_slots[&first_key.hashed_key()], H256::repeat_byte(3));
        assert_eq!(
            touched_slots[&second_key.hashed_key()],
            H256::repeat_byte(2)
        );

        test_revert(&mut conn, first_key, second_key).await;
    }

    async fn test_revert(conn: &mut Connection<'_, Core>, key: StorageKey, second_key: StorageKey) {
        let new_account = AccountTreeId::new(Address::repeat_byte(2));
        let new_key = StorageKey::new(new_account, H256::zero());
        let log = StorageLog::new_write_log(key, H256::repeat_byte(0xff));
        let other_log = StorageLog::new_write_log(second_key, H256::zero());
        let new_key_log = StorageLog::new_write_log(new_key, H256::repeat_byte(0xfe));
        let logs = vec![log, other_log, new_key_log];
        insert_l2_block(conn, 2, logs).await;

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

        conn.storage_logs_dal()
            .roll_back_storage_logs(L2BlockNumber(1))
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
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let logs: Vec<_> = (0_u8..10)
            .map(|i| {
                let key = StorageKey::new(account, H256::from_low_u64_be(u64::from(i)));
                StorageLog::new_write_log(key, H256::repeat_byte(i))
            })
            .collect();
        insert_l2_block(&mut conn, 1, logs.clone()).await;
        let written_keys: Vec<_> = logs.iter().map(|log| log.key.hashed_key()).collect();
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
        insert_l2_block(&mut conn, 2, new_logs.clone()).await;
        let new_written_keys: Vec<_> = new_logs[5..]
            .iter()
            .map(|log| log.key.hashed_key())
            .collect();
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
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

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
            insert_l2_block(&mut conn, l1_batch, logs.clone()).await;

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
                    let hashed_key = log.key.hashed_key();
                    (!log.value.is_zero() && !non_initial.contains(&hashed_key))
                        .then_some(hashed_key)
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
            .get_chunk_starts_for_l2_block(L2BlockNumber(1), &key_ranges)
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
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let logs: Vec<_> = (0..count)
            .map(|i| {
                let key = StorageKey::new(account, H256::repeat_byte(i));
                StorageLog::new_write_log(key, H256::repeat_byte(i))
            })
            .collect();
        insert_l2_block(conn, 1, logs.clone()).await;

        let mut initial_keys: Vec<_> = logs.iter().map(|log| log.key).collect();
        initial_keys.sort_unstable();
        let initial_keys: Vec<_> = initial_keys.iter().map(StorageKey::hashed_key).collect();
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
            .get_tree_entries_for_l2_block(L2BlockNumber(1), key_range)
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
            .get_tree_entries_for_l2_block(L2BlockNumber(1), key_range.clone())
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
        let bytecode_hash = H256::repeat_byte(0xff);
        let successful_deployment =
            StorageLog::new_write_log(get_code_key(&contract_address), bytecode_hash);
        let failed_deployment = StorageLog::new_write_log(
            get_code_key(&contract_address),
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
        );

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        // If deployment fails then two writes are issued, one that writes `bytecode_hash` to the "correct" value,
        // and the next write reverts its value back to `FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH`.
        insert_l2_block(&mut conn, 1, vec![successful_deployment, failed_deployment]).await;

        let tested_l2_blocks = [
            None,
            Some(L2BlockNumber(0)),
            Some(L2BlockNumber(1)),
            Some(L2BlockNumber(1)),
        ];
        for at_l2_block in tested_l2_blocks {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    at_l2_block,
                )
                .await
                .unwrap();
            assert!(
                deployed_map.is_empty(),
                "{deployed_map:?} at L2 block {at_l2_block:?}"
            );
        }

        insert_l2_block(&mut conn, 2, vec![successful_deployment]).await;

        for old_l2_block in [L2BlockNumber(0), L2BlockNumber(1)] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    Some(old_l2_block),
                )
                .await
                .unwrap();
            assert!(
                deployed_map.is_empty(),
                "{deployed_map:?} at {old_l2_block}"
            );
        }
        for new_l2_block in [None, Some(L2BlockNumber(2))] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    new_l2_block,
                )
                .await
                .unwrap();
            assert_eq!(
                deployed_map,
                HashMap::from([(contract_address, (L2BlockNumber(2), bytecode_hash))])
            );
        }

        let other_successful_deployment = StorageLog::new_write_log(
            get_code_key(&other_contract_address),
            H256::repeat_byte(0xff),
        );
        insert_l2_block(&mut conn, 3, vec![other_successful_deployment]).await;

        for old_l2_block in [L2BlockNumber(0), L2BlockNumber(1)] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    Some(old_l2_block),
                )
                .await
                .unwrap();
            assert!(
                deployed_map.is_empty(),
                "{deployed_map:?} at L2 block {old_l2_block}"
            );
        }

        let deployed_map = conn
            .storage_logs_dal()
            .filter_deployed_contracts(
                [contract_address, other_contract_address].into_iter(),
                Some(L2BlockNumber(2)),
            )
            .await
            .unwrap();
        assert_eq!(
            deployed_map,
            HashMap::from([(contract_address, (L2BlockNumber(2), bytecode_hash))])
        );

        for new_l2_block in [None, Some(L2BlockNumber(3))] {
            let deployed_map = conn
                .storage_logs_dal()
                .filter_deployed_contracts(
                    [contract_address, other_contract_address].into_iter(),
                    new_l2_block,
                )
                .await
                .unwrap();
            assert_eq!(
                deployed_map,
                HashMap::from([
                    (contract_address, (L2BlockNumber(2), bytecode_hash)),
                    (other_contract_address, (L2BlockNumber(3), bytecode_hash)),
                ])
            );
        }
    }
}
