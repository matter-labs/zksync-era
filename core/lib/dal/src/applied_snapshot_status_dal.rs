use zksync_types::{snapshots::AppliedSnapshotStatus, L1BatchNumber, MiniblockNumber};

use crate::StorageProcessor;

#[derive(Debug)]
pub struct AppliedSnapshotStatusDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl AppliedSnapshotStatusDal<'_, '_> {
    pub async fn set_applied_snapshot_status(
        &mut self,
        status: &AppliedSnapshotStatus,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                snapshot_recovery (
                    l1_batch_number,
                    l1_batch_root_hash,
                    miniblock_number,
                    miniblock_root_hash,
                    is_finished,
                    last_finished_chunk_id,
                    total_chunk_count,
                    updated_at,
                    created_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
                l1_batch_number = excluded.l1_batch_number,
                l1_batch_root_hash = excluded.l1_batch_root_hash,
                miniblock_number = excluded.miniblock_number,
                miniblock_root_hash = excluded.miniblock_root_hash,
                is_finished = excluded.is_finished,
                last_finished_chunk_id = excluded.last_finished_chunk_id,
                total_chunk_count = excluded.total_chunk_count,
                updated_at = excluded.updated_at
            "#,
            status.l1_batch_number.0 as i64,
            status.l1_batch_root_hash,
            status.miniblock_number.0 as i64,
            status.miniblock_root_hash,
            status.is_finished,
            status.last_finished_chunk_id.map(|v| v as i32),
            status.total_chunk_count as i64,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_applied_snapshot_status(
        &mut self,
    ) -> sqlx::Result<Option<AppliedSnapshotStatus>> {
        let record = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                l1_batch_root_hash,
                miniblock_number,
                miniblock_root_hash,
                is_finished,
                last_finished_chunk_id,
                total_chunk_count
            FROM
                snapshot_recovery
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(record.map(|r| AppliedSnapshotStatus {
            l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
            l1_batch_root_hash: r.l1_batch_root_hash,
            miniblock_number: MiniblockNumber(r.miniblock_number as u32),
            miniblock_root_hash: r.miniblock_root_hash,
            is_finished: r.is_finished,
            last_finished_chunk_id: r.last_finished_chunk_id.map(|v| v as u64),
            total_chunk_count: r.total_chunk_count as u64,
        }))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{snapshots::AppliedSnapshotStatus, L1BatchNumber, MiniblockNumber};

    use crate::ConnectionPool;

    #[tokio::test]
    async fn resolving_earliest_block_id() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        let mut applied_status_dal = conn.applied_snapshot_status_dal();
        let empty_status = applied_status_dal
            .get_applied_snapshot_status()
            .await
            .unwrap();
        assert_eq!(None, empty_status);
        let status = AppliedSnapshotStatus {
            l1_batch_number: L1BatchNumber(123),
            l1_batch_root_hash: vec![1, 52, 68, 123, 255],
            miniblock_number: MiniblockNumber(234),
            miniblock_root_hash: vec![31, 95, 12, 3, 71],
            is_finished: false,
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
        assert_eq!(Some(status), status_from_db);

        let updated_status = AppliedSnapshotStatus {
            l1_batch_number: L1BatchNumber(123),
            l1_batch_root_hash: vec![155, 161, 85, 8, 19],
            miniblock_number: MiniblockNumber(234),
            miniblock_root_hash: vec![82, 18, 61, 63, 90],
            is_finished: true,
            last_finished_chunk_id: Some(2345),
            total_chunk_count: 345,
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
