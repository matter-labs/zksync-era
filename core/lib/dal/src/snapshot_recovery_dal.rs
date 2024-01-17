use zksync_types::{
    snapshots::SnapshotRecoveryStatus, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256,
};

use crate::StorageProcessor;

#[derive(Debug)]
pub struct SnapshotRecoveryDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotRecoveryDal<'_, '_> {
    pub async fn set_applied_snapshot_status(
        &mut self,
        status: &SnapshotRecoveryStatus,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                snapshot_recovery (
                    l1_batch_number,
                    l1_batch_timestamp,
                    l1_batch_root_hash,
                    miniblock_number,
                    miniblock_timestamp,
                    miniblock_hash,
                    protocol_version,
                    last_finished_chunk_id,
                    total_chunk_count,
                    updated_at,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
                l1_batch_number = excluded.l1_batch_number,
                l1_batch_timestamp = excluded.l1_batch_timestamp,
                l1_batch_root_hash = excluded.l1_batch_root_hash,
                miniblock_number = excluded.miniblock_number,
                miniblock_timestamp = excluded.miniblock_timestamp,
                miniblock_hash = excluded.miniblock_hash,
                protocol_version = excluded.protocol_version,
                last_finished_chunk_id = excluded.last_finished_chunk_id,
                total_chunk_count = excluded.total_chunk_count,
                updated_at = excluded.updated_at
            "#,
            status.l1_batch_number.0 as i64,
            status.l1_batch_timestamp as i64,
            status.l1_batch_root_hash.0.as_slice(),
            status.miniblock_number.0 as i64,
            status.miniblock_timestamp as i64,
            status.miniblock_hash.0.as_slice(),
            status.protocol_version as i32,
            status.last_finished_chunk_id.map(|v| v as i32),
            status.total_chunk_count as i64,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_applied_snapshot_status(
        &mut self,
    ) -> sqlx::Result<Option<SnapshotRecoveryStatus>> {
        let record = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                l1_batch_timestamp,
                l1_batch_root_hash,
                miniblock_number,
                miniblock_timestamp,
                miniblock_hash,
                protocol_version,
                last_finished_chunk_id,
                total_chunk_count
            FROM
                snapshot_recovery
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(record.map(|row| SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
            l1_batch_timestamp: row.l1_batch_timestamp as u64,
            l1_batch_root_hash: H256::from_slice(&row.l1_batch_root_hash),
            miniblock_number: MiniblockNumber(row.miniblock_number as u32),
            miniblock_timestamp: row.miniblock_timestamp as u64,
            miniblock_hash: H256::from_slice(&row.miniblock_hash),
            protocol_version: ProtocolVersionId::try_from(row.protocol_version as u16).unwrap(),
            last_finished_chunk_id: row.last_finished_chunk_id.map(|v| v as u64),
            total_chunk_count: row.total_chunk_count as u64,
        }))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        snapshots::SnapshotRecoveryStatus, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256,
    };

    use crate::ConnectionPool;

    #[tokio::test]
    async fn resolving_earliest_block_id() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        let mut applied_status_dal = conn.snapshot_recovery_dal();
        let empty_status = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(None, empty_status);
        let status = SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(123),
            l1_batch_timestamp: 123,
            l1_batch_root_hash: H256::random(),
            miniblock_number: MiniblockNumber(234),
            miniblock_timestamp: 234,
            miniblock_hash: H256::random(),
            protocol_version: ProtocolVersionId::latest(),
            last_finished_chunk_id: None,
            total_chunk_count: 345,
        };
        applied_status_dal
            .set_applied_snapshot_status(&status)
            .await
            .unwrap();
        let status_from_db = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(status, status_from_db.unwrap());

        let updated_status = SnapshotRecoveryStatus {
            last_finished_chunk_id: Some(2345),
            total_chunk_count: 345,
            ..status
        };
        applied_status_dal
            .set_applied_snapshot_status(&updated_status)
            .await
            .unwrap();
        let updated_status_from_db = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(updated_status, updated_status_from_db.unwrap());
    }
}
