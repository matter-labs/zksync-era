use zksync_types::{
    snapshots::{SnapshotFactoryDependency, SnapshotStorageLog},
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, StorageKey, H256,
};

use crate::{instrument::InstrumentExt, StorageProcessor};

#[derive(Debug)]
pub struct SnapshotsCreatorDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotsCreatorDal<'_, '_> {
    pub async fn get_distinct_storage_logs_keys_count(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<u64> {
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
        .fetch_one(self.storage.conn())
        .await?
        .index;
        Ok(count as u64)
    }

    pub async fn get_storage_logs_chunk(
        &mut self,
        miniblock_number: MiniblockNumber,
        hashed_keys_range: std::ops::RangeInclusive<H256>,
    ) -> sqlx::Result<Vec<SnapshotStorageLog>> {
        let storage_logs = sqlx::query!(
            r#"
            SELECT
                storage_logs.key AS "key!",
                storage_logs.value AS "value!",
                storage_logs.address AS "address!",
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
                        AND hashed_key >= $2
                        AND hashed_key < $3
                    GROUP BY
                        hashed_key
                    ORDER BY
                        hashed_key
                ) AS keys
                INNER JOIN storage_logs ON keys.hashed_key = storage_logs.hashed_key
                AND storage_logs.miniblock_number = keys.op[1]
                AND storage_logs.operation_number = keys.op[2]
                INNER JOIN initial_writes ON keys.hashed_key = initial_writes.hashed_key;
            "#,
            miniblock_number.0 as i64,
            hashed_keys_range.start().0.as_slice(),
            hashed_keys_range.end().0.as_slice(),
        )
        .instrument("get_storage_logs_chunk")
        .with_arg("miniblock_number", &miniblock_number)
        .with_arg("min_hashed_key", &hashed_keys_range.start())
        .with_arg("max_hashed_key", &hashed_keys_range.end())
        .report_latency()
        .fetch_all(self.storage.conn())
        .await?
        .iter()
        .map(|row| SnapshotStorageLog {
            key: StorageKey::new(
                AccountTreeId::new(Address::from_slice(&row.address)),
                H256::from_slice(&row.key),
            ),
            value: H256::from_slice(&row.value),
            l1_batch_number_of_initial_write: L1BatchNumber(row.l1_batch_number as u32),
            enumeration_index: row.index.unwrap() as u64,
        })
        .collect();
        Ok(storage_logs)
    }

    pub async fn get_all_factory_deps(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> sqlx::Result<Vec<SnapshotFactoryDependency>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                bytecode
            FROM
                factory_deps
            WHERE
                miniblock_number <= $1
            "#,
            miniblock_number.0 as i64,
        )
        .instrument("get_all_factory_deps")
        .report_latency()
        .fetch_all(self.storage.conn())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SnapshotFactoryDependency {
                bytecode: row.bytecode.into(),
            })
            .collect())
    }
}
