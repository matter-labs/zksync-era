use zksync_types::{
    snapshots::SnapshotRecoveryStatus, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256,
};

use crate::StorageProcessor;

#[derive(Debug)]
pub struct SnapshotRecoveryDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotRecoveryDal<'_, '_> {
    pub async fn insert_initial_recovery_status(
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
                    storage_logs_chunks_processed,
                    updated_at,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
            "#,
            status.l1_batch_number.0 as i64,
            status.l1_batch_timestamp as i64,
            status.l1_batch_root_hash.0.as_slice(),
            status.miniblock_number.0 as i64,
            status.miniblock_timestamp as i64,
            status.miniblock_hash.0.as_slice(),
            status.protocol_version as i32,
            &status.storage_logs_chunks_processed,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn mark_storage_logs_chunk_as_processed(
        &mut self,
        chunk_id: u64,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE snapshot_recovery
            SET
                storage_logs_chunks_processed[$1] = TRUE,
                updated_at = NOW()
            "#,
            chunk_id as i32 + 1
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
                storage_logs_chunks_processed
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
            storage_logs_chunks_processed: row.storage_logs_chunks_processed,
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
    async fn manipulating_snapshot_recovery_table() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        let mut applied_status_dal = conn.snapshot_recovery_dal();
        let empty_status = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(None, empty_status);
        let mut status = SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(123),
            l1_batch_timestamp: 123,
            l1_batch_root_hash: H256::random(),
            miniblock_number: MiniblockNumber(234),
            miniblock_timestamp: 234,
            miniblock_hash: H256::random(),
            protocol_version: ProtocolVersionId::latest(),
            storage_logs_chunks_processed: vec![false, false, true, false],
        };
        applied_status_dal
            .insert_initial_recovery_status(&status)
            .await
            .unwrap();
        let status_from_db = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(status, status_from_db.unwrap());

        status.storage_logs_chunks_processed = vec![false, true, true, true];
        applied_status_dal
            .mark_storage_logs_chunk_as_processed(1)
            .await
            .unwrap();
        applied_status_dal
            .mark_storage_logs_chunk_as_processed(3)
            .await
            .unwrap();

        let updated_status_from_db = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(status, updated_status_from_db.unwrap());
    }
}
