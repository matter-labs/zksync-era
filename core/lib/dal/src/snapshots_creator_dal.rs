use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{
    snapshots::SnapshotStorageLog, AccountTreeId, Address, L1BatchNumber, L2BlockNumber,
    StorageKey, H256,
};

use crate::Core;

#[derive(Debug)]
pub struct SnapshotsCreatorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl SnapshotsCreatorDal<'_, '_> {
    pub async fn get_distinct_storage_logs_keys_count(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<u64> {
        let count = sqlx::query!(
            r#"
            SELECT
                INDEX
            FROM
                initial_writes
            WHERE
                l1_batch_number <= $1
            ORDER BY
                l1_batch_number DESC,
                INDEX DESC
            LIMIT
                1;
            "#,
            l1_batch_number.0 as i32
        )
        .instrument("get_storage_logs_count")
        .report_latency()
        .expect_slow_query()
        .fetch_one(self.storage)
        .await?
        .index;
        Ok(count as u64)
    }

    /// Constructs a `storate_logs` chunk of the state AFTER processing `[0..l1_batch_number]`
    /// batches. `l2_block_number` MUST be the last L2 block of the `l1_batch_number` batch.
    pub async fn get_storage_logs_chunk(
        &mut self,
        l2_block_number: L2BlockNumber,
        l1_batch_number: L1BatchNumber,
        hashed_keys_range: std::ops::RangeInclusive<H256>,
    ) -> DalResult<Vec<SnapshotStorageLog>> {
        // We need to filter the returned logs by `l1_batch_number` in order to not return "phantom writes", i.e.,
        // logs that have deduplicated writes (e.g., a write to a non-zero value and back to zero in the same L1 batch)
        // which are actually written to in future L1 batches.
        let storage_logs = sqlx::query!(
            r#"
            SELECT
                storage_logs.hashed_key AS "hashed_key!",
                storage_logs.value AS "value!",
                storage_logs.miniblock_number AS "miniblock_number!",
                initial_writes.l1_batch_number AS "l1_batch_number!",
                initial_writes.index
            FROM
                (
                    SELECT
                        hashed_key,
                        MAX(ARRAY[miniblock_number, operation_number]::INT[]) AS op
                    FROM
                        storage_logs
                    WHERE
                        miniblock_number <= $1
                        AND hashed_key >= $3
                        AND hashed_key <= $4
                    GROUP BY
                        hashed_key
                    ORDER BY
                        hashed_key
                ) AS keys
                INNER JOIN storage_logs ON keys.hashed_key = storage_logs.hashed_key
                AND storage_logs.miniblock_number = keys.op[1]
                AND storage_logs.operation_number = keys.op[2]
                INNER JOIN initial_writes ON keys.hashed_key = initial_writes.hashed_key
            WHERE
                initial_writes.l1_batch_number <= $2
            "#,
            i64::from(l2_block_number.0),
            i64::from(l1_batch_number.0),
            hashed_keys_range.start().as_bytes(),
            hashed_keys_range.end().as_bytes()
        )
        .instrument("get_storage_logs_chunk")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("min_hashed_key", &hashed_keys_range.start())
        .with_arg("max_hashed_key", &hashed_keys_range.end())
        .report_latency()
        .expect_slow_query()
        .fetch_all(self.storage)
        .await?
        .iter()
        .map(|row| SnapshotStorageLog {
            key: H256::from_slice(&row.hashed_key),
            value: H256::from_slice(&row.value),
            l1_batch_number_of_initial_write: L1BatchNumber(row.l1_batch_number as u32),
            enumeration_index: row.index as u64,
        })
        .collect();
        Ok(storage_logs)
    }

    /// Same as [`Self::get_storage_logs_chunk()`], but returns full keys.
    #[deprecated(
        note = "will fail if called on a node restored from a v1 snapshot; use `get_storage_logs_chunk()` instead"
    )]
    pub async fn get_storage_logs_chunk_with_key_preimages(
        &mut self,
        l2_block_number: L2BlockNumber,
        l1_batch_number: L1BatchNumber,
        hashed_keys_range: std::ops::RangeInclusive<H256>,
    ) -> DalResult<Vec<SnapshotStorageLog<StorageKey>>> {
        let storage_logs = sqlx::query!(
            r#"
            SELECT
                storage_logs.address AS "address!",
                storage_logs.key AS "key!",
                storage_logs.value AS "value!",
                storage_logs.miniblock_number AS "miniblock_number!",
                initial_writes.l1_batch_number AS "l1_batch_number!",
                initial_writes.index
            FROM
                (
                    SELECT
                        hashed_key,
                        MAX(ARRAY[miniblock_number, operation_number]::INT[]) AS op
                    FROM
                        storage_logs
                    WHERE
                        miniblock_number <= $1
                        AND hashed_key >= $3
                        AND hashed_key <= $4
                    GROUP BY
                        hashed_key
                    ORDER BY
                        hashed_key
                ) AS keys
                INNER JOIN storage_logs ON keys.hashed_key = storage_logs.hashed_key
                AND storage_logs.miniblock_number = keys.op[1]
                AND storage_logs.operation_number = keys.op[2]
                INNER JOIN initial_writes ON keys.hashed_key = initial_writes.hashed_key
            WHERE
                initial_writes.l1_batch_number <= $2
            "#,
            i64::from(l2_block_number.0),
            i64::from(l1_batch_number.0),
            hashed_keys_range.start().as_bytes(),
            hashed_keys_range.end().as_bytes()
        )
        .instrument("get_storage_logs_chunk")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("min_hashed_key", &hashed_keys_range.start())
        .with_arg("max_hashed_key", &hashed_keys_range.end())
        .report_latency()
        .expect_slow_query()
        .fetch_all(self.storage)
        .await?
        .iter()
        .map(|row| SnapshotStorageLog {
            key: StorageKey::new(
                AccountTreeId::new(Address::from_slice(&row.address)),
                H256::from_slice(&row.key),
            ),
            value: H256::from_slice(&row.value),
            l1_batch_number_of_initial_write: L1BatchNumber(row.l1_batch_number as u32),
            enumeration_index: row.index as u64,
        })
        .collect();
        Ok(storage_logs)
    }

    /// Returns all factory dependencies up to and including the specified `l2_block_number`.
    pub async fn get_all_factory_deps(
        &mut self,
        l2_block_number: L2BlockNumber,
    ) -> DalResult<Vec<(H256, Vec<u8>)>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                bytecode_hash,
                bytecode
            FROM
                factory_deps
            WHERE
                miniblock_number <= $1
            "#,
            i64::from(l2_block_number.0),
        )
        .instrument("get_all_factory_deps")
        .report_latency()
        .expect_slow_query()
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (H256::from_slice(&row.bytecode_hash), row.bytecode))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::StorageLog;

    use super::*;
    use crate::{ConnectionPool, Core, CoreDal};

    #[tokio::test]
    async fn getting_storage_log_chunks_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let logs = (0..100).map(|i| {
            let key = StorageKey::new(
                AccountTreeId::new(Address::random()),
                H256::from_low_u64_be(i),
            );
            StorageLog::new_write_log(key, H256::repeat_byte(1))
        });
        let mut logs: Vec<_> = logs.collect();
        logs.sort_unstable_by_key(|log| log.key.hashed_key());

        conn.storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(1), &logs)
            .await
            .unwrap();
        let mut written_keys: Vec<_> = logs.iter().map(|log| log.key).collect();
        written_keys.sort_unstable();
        let written_keys: Vec<_> = written_keys.iter().map(StorageKey::hashed_key).collect();
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(1), &written_keys)
            .await
            .unwrap();

        let log_row_count = conn
            .storage_logs_dal()
            .get_storage_logs_row_count(L2BlockNumber(1))
            .await
            .unwrap();
        assert_eq!(log_row_count, logs.len() as u64);
        assert_logs_for_snapshot(&mut conn, L2BlockNumber(1), L1BatchNumber(1), &logs).await;

        // Add some inserts / updates in the next L2 block. They should be ignored.
        let new_logs = (100..150).map(|i| {
            let key = StorageKey::new(
                AccountTreeId::new(Address::random()),
                H256::from_low_u64_be(i),
            );
            StorageLog::new_write_log(key, H256::repeat_byte(1))
        });
        let new_written_keys: Vec<_> = new_logs.clone().map(|log| log.key.hashed_key()).collect();
        let updated_logs = logs.iter().step_by(3).map(|&log| StorageLog {
            value: H256::repeat_byte(23),
            ..log
        });
        let all_new_logs: Vec<_> = new_logs.chain(updated_logs).collect();
        let all_new_logs_len = all_new_logs.len();
        conn.storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(2), &all_new_logs)
            .await
            .unwrap();
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(2), &new_written_keys)
            .await
            .unwrap();

        let log_row_count = conn
            .storage_logs_dal()
            .get_storage_logs_row_count(L2BlockNumber(1))
            .await
            .unwrap();
        assert_eq!(log_row_count, logs.len() as u64);
        let log_row_count = conn
            .storage_logs_dal()
            .get_storage_logs_row_count(L2BlockNumber(2))
            .await
            .unwrap();
        assert_eq!(log_row_count, (logs.len() + all_new_logs_len) as u64);
        assert_logs_for_snapshot(&mut conn, L2BlockNumber(1), L1BatchNumber(1), &logs).await;
    }

    async fn assert_logs_for_snapshot(
        conn: &mut Connection<'_, Core>,
        l2_block_number: L2BlockNumber,
        l1_batch_number: L1BatchNumber,
        expected_logs: &[StorageLog],
    ) {
        let all_logs = conn
            .snapshots_creator_dal()
            .get_storage_logs_chunk(
                l2_block_number,
                l1_batch_number,
                H256::zero()..=H256::repeat_byte(0xff),
            )
            .await
            .unwrap();
        assert_eq!(all_logs.len(), expected_logs.len());
        for (log, expected_log) in all_logs.iter().zip(expected_logs) {
            assert_eq!(log.key, expected_log.key.hashed_key());
            assert_eq!(log.value, expected_log.value);
            assert_eq!(log.l1_batch_number_of_initial_write, l1_batch_number);
        }

        for chunk_size in [2, 5, expected_logs.len() / 3] {
            for chunk in expected_logs.chunks(chunk_size) {
                let range = chunk[0].key.hashed_key()..=chunk.last().unwrap().key.hashed_key();
                let logs = conn
                    .snapshots_creator_dal()
                    .get_storage_logs_chunk(l2_block_number, l1_batch_number, range)
                    .await
                    .unwrap();
                assert_eq!(logs.len(), chunk.len());
                for (log, expected_log) in logs.iter().zip(chunk) {
                    assert_eq!(log.key, expected_log.key.hashed_key());
                    assert_eq!(log.value, expected_log.value);
                }
            }
        }
    }

    #[tokio::test]
    async fn phantom_writes_are_filtered_out() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let key = StorageKey::new(AccountTreeId::default(), H256::repeat_byte(1));
        let phantom_writes = vec![
            StorageLog::new_write_log(key, H256::repeat_byte(1)),
            StorageLog::new_write_log(key, H256::zero()),
        ];
        conn.storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(1), &phantom_writes)
            .await
            .unwrap();
        // initial writes are intentionally not inserted.

        let real_write = StorageLog::new_write_log(key, H256::repeat_byte(2));
        conn.storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(2), &[real_write])
            .await
            .unwrap();
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(2), &[key.hashed_key()])
            .await
            .unwrap();

        let logs = conn
            .snapshots_creator_dal()
            .get_storage_logs_chunk(
                L2BlockNumber(1),
                L1BatchNumber(1),
                H256::zero()..=H256::repeat_byte(0xff),
            )
            .await
            .unwrap();
        assert_eq!(logs, []);

        let logs = conn
            .snapshots_creator_dal()
            .get_storage_logs_chunk(
                L2BlockNumber(2),
                L1BatchNumber(2),
                H256::zero()..=H256::repeat_byte(0xff),
            )
            .await
            .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].key, key.hashed_key());
        assert_eq!(logs[0].value, real_write.value);
        assert_eq!(logs[0].l1_batch_number_of_initial_write, L1BatchNumber(2));
    }
}
