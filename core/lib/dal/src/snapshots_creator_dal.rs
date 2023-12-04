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
    ) -> sqlx::Result<u64> {
        let count = sqlx::query!(
            r#"
            SELECT index
            FROM initial_writes
            WHERE l1_batch_number <= $1
            ORDER BY l1_batch_number DESC , index DESC 
            LIMIT 1;
            "#,
            l1_batch_number.0 as i32
        )
        .instrument("get_storage_logs_count")
        .report_latency()
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .index;
        Ok(count as u64)
    }

    pub async fn get_storage_logs_chunk(
        &mut self,
        miniblock_number: MiniblockNumber,
        min_hashed_key: &[u8],
        max_hashed_key: &[u8],
    ) -> sqlx::Result<Vec<SnapshotStorageLog>> {
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
                  WHERE miniblock_number <= $1 and hashed_key >= $2 and hashed_key < $3
                  GROUP BY hashed_key
                  ORDER BY hashed_key) AS keys
                     INNER JOIN storage_logs ON keys.hashed_key = storage_logs.hashed_key
                AND storage_logs.miniblock_number = keys.op[1]
                AND storage_logs.operation_number = keys.op[2]
                     INNER JOIN initial_writes ON keys.hashed_key = initial_writes.hashed_key;
             "#,
            miniblock_number.0 as i64,
            min_hashed_key,
            max_hashed_key,
        )
        .instrument("get_storage_logs_chunk")
        .report_latency()
        .fetch_all(self.storage.conn())
        .await?
        .iter()
        .map(|row| SnapshotStorageLog {
            key: StorageKey::new(
                AccountTreeId::new(Address::from_slice(row.address.as_ref().unwrap())),
                H256::from_slice(row.key.as_ref().unwrap()),
            ),
            value: H256::from_slice(row.value.as_ref().unwrap()),
            l1_batch_number_of_initial_write: L1BatchNumber(row.l1_batch_number.unwrap() as u32),
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
            "SELECT bytecode, bytecode_hash FROM factory_deps WHERE miniblock_number <= $1",
            miniblock_number.0 as i64,
        )
        .instrument("get_all_factory_deps")
        .report_latency()
        .fetch_all(self.storage.conn())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SnapshotFactoryDependency {
                bytecode: row.bytecode,
            })
            .collect())
    }
}
