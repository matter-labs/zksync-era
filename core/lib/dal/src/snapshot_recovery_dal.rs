use zksync_types::{snapshots::SnapshotRecoveryStatus, L1BatchNumber, MiniblockNumber, H256};

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
                    l1_batch_root_hash,
                    miniblock_number,
                    miniblock_root_hash,
                    storage_logs_chunks_processed,
                    updated_at,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, NOW(), NOW())
            "#,
            status.l1_batch_number.0 as i64,
            status.l1_batch_root_hash.0.as_slice(),
            status.miniblock_number.0 as i64,
            status.miniblock_root_hash.0.as_slice(),
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
                l1_batch_root_hash,
                miniblock_number,
                miniblock_root_hash,
                storage_logs_chunks_processed
            FROM
                snapshot_recovery
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(record.map(|r| SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
            l1_batch_root_hash: H256::from_slice(&r.l1_batch_root_hash),
            miniblock_number: MiniblockNumber(r.miniblock_number as u32),
            miniblock_root_hash: H256::from_slice(&r.miniblock_root_hash),
            storage_logs_chunks_processed: r.storage_logs_chunks_processed.into_iter().collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{snapshots::SnapshotRecoveryStatus, L1BatchNumber, MiniblockNumber, H256};

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
            l1_batch_root_hash: H256::random(),
            miniblock_number: MiniblockNumber(234),
            miniblock_root_hash: H256::random(),
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
