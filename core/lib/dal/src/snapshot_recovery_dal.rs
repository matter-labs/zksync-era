use zksync_types::{snapshots::SnapshotRecoveryStatus, L1BatchNumber, MiniblockNumber, H256};

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
        let to_process: Vec<_> = status
            .storage_logs_chunks_ids_to_process
            .clone()
            .into_iter()
            .map(|v| v as i32)
            .collect();
        let already_processed: Vec<_> = status
            .storage_logs_chunks_ids_already_processed
            .clone()
            .into_iter()
            .map(|v| v as i32)
            .collect();
        sqlx::query!(
            r#"
            INSERT INTO
                snapshot_recovery (
                    l1_batch_number,
                    l1_batch_root_hash,
                    miniblock_number,
                    miniblock_root_hash,
                    storage_logs_chunks_ids_to_process,
                    storage_logs_chunks_ids_already_processed,
                    updated_at,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
                l1_batch_number = excluded.l1_batch_number,
                l1_batch_root_hash = excluded.l1_batch_root_hash,
                miniblock_number = excluded.miniblock_number,
                miniblock_root_hash = excluded.miniblock_root_hash,
                storage_logs_chunks_ids_to_process = excluded.storage_logs_chunks_ids_to_process,
                storage_logs_chunks_ids_already_processed = excluded.storage_logs_chunks_ids_already_processed,
                updated_at = excluded.updated_at
            "#,
            status.l1_batch_number.0 as i64,
            status.l1_batch_root_hash.0.as_slice(),
            status.miniblock_number.0 as i64,
            status.miniblock_root_hash.0.as_slice(),
            &to_process,
            &already_processed
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
                storage_logs_chunks_ids_to_process,
                storage_logs_chunks_ids_already_processed
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
            storage_logs_chunks_ids_to_process: r
                .storage_logs_chunks_ids_to_process
                .into_iter()
                .map(|v| v as u64)
                .collect(),
            storage_logs_chunks_ids_already_processed: r
                .storage_logs_chunks_ids_already_processed
                .into_iter()
                .map(|v| v as u64)
                .collect(),
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
        let status = SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(123),
            l1_batch_root_hash: H256::random(),
            miniblock_number: MiniblockNumber(234),
            miniblock_root_hash: H256::random(),
            storage_logs_chunks_ids_to_process: vec![1, 2, 3, 4],
            storage_logs_chunks_ids_already_processed: vec![5],
        };
        applied_status_dal
            .set_applied_snapshot_status(&status)
            .await
            .unwrap();
        let status_from_db = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(Some(status), status_from_db);

        let updated_status = SnapshotRecoveryStatus {
            l1_batch_number: L1BatchNumber(123),
            l1_batch_root_hash: H256::random(),
            miniblock_number: MiniblockNumber(234),
            miniblock_root_hash: H256::random(),
            storage_logs_chunks_ids_to_process: vec![2, 3],
            storage_logs_chunks_ids_already_processed: vec![1, 4, 5],
        };
        applied_status_dal
            .set_applied_snapshot_status(&updated_status)
            .await
            .unwrap();
        let updated_status_from_db = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(Some(updated_status), updated_status_from_db);
    }
}
