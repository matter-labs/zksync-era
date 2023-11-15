use crate::instrument::InstrumentExt;
use crate::StorageProcessor;
use zksync_types::snapshots::{SnapshotFactoryDependency, SnapshotStorageLog};
use zksync_types::{AccountTreeId, Address, L1BatchNumber, MiniblockNumber, StorageKey, H256};

#[derive(Debug)]
pub struct SnapshotsCreatorDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotsCreatorDal<'_, '_> {
    pub async fn get_storage_logs_count(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<u64, sqlx::Error> {
        let count = sqlx::query!(
            "SELECT count(*) FROM initial_writes WHERE l1_batch_number <= $1",
            l1_batch_number.0 as i32
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .count
        .unwrap();
        Ok(count as u64)
    }

    pub async fn get_storage_logs_chunk(
        &mut self,
        l1_batch_number: L1BatchNumber,
        chunk_id: u64,
        chunk_size: u64,
    ) -> Result<Vec<SnapshotStorageLog>, sqlx::Error> {
        let miniblock_number: i64 = sqlx::query!(
            "select MAX(number) from miniblocks where l1_batch_number = $1",
            l1_batch_number.0 as i64
        )
        .fetch_one(self.storage.conn())
        .await?
        .max
        .unwrap_or_default();

        let storage_logs = sqlx::query!(
            r#"
            SELECT storage_logs.key,
                   storage_logs.value,
                   storage_logs.address,
                   storage_logs.miniblock_number,
                   initial_writes.l1_batch_number,
                   initial_writes.index
            FROM (SELECT hashed_key,
                         max(ARRAY [miniblock_number, operation_number]::int[]) AS op
                  FROM storage_logs
                  WHERE miniblock_number <= $1
                  GROUP BY hashed_key
                  ORDER BY hashed_key) AS keys
                     INNER JOIN storage_logs ON keys.hashed_key = storage_logs.hashed_key
                AND storage_logs.miniblock_number = keys.op[1]
                AND storage_logs.operation_number = keys.op[2]
                     INNER JOIN initial_writes ON keys.hashed_key = initial_writes.hashed_key
            WHERE miniblock_number <= $1
            ORDER BY hashed_key
            LIMIT $2 OFFSET $3;
             "#,
            miniblock_number,
            chunk_size as i64,
            (chunk_size * chunk_id) as i64
        )
        .instrument("get_storage_logs_chunk")
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
            enumeration_index: row.index as u64,
        })
        .collect();
        Ok(storage_logs)
    }

    pub async fn get_all_factory_deps(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Vec<SnapshotFactoryDependency> {
        sqlx::query!(
            "SELECT bytecode, bytecode_hash FROM factory_deps WHERE miniblock_number <= $1",
            miniblock_number.0 as i64,
        )
        .instrument("get_all_factory_deps")
        .report_latency()
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| SnapshotFactoryDependency {
            bytecode_hash: H256::from_slice(&row.bytecode_hash),
            bytecode: row.bytecode,
        })
        .collect()
    }
}
