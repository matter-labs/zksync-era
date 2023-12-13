use zksync_types::{snapshots::AppliedSnapshotStatus, L1BatchNumber};

use crate::StorageProcessor;

#[derive(Debug)]
pub struct AppliedSnapshotStatusDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl AppliedSnapshotStatusDal<'_, '_> {
    pub async fn set_applied_snapshot_status(
        &mut self,
        status: &AppliedSnapshotStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO applied_snapshot_status(l1_batch_number, is_finished, last_finished_chunk_id)
            VALUES ($1, $2, $3)
            ON CONFLICT
                (l1_batch_number)
                DO UPDATE
                SET l1_batch_number        = excluded.l1_batch_number,
                    is_finished            = excluded.is_finished,
                    last_finished_chunk_id = excluded.last_finished_chunk_id
            "#,
            status.l1_batch_number.0 as i64,
            status.is_finished,
            status.last_finished_chunk_id.map(|v| v as i32)
        )
            .execute(self.storage.conn())
            .await
            .unwrap();
        Ok(())
    }

    pub async fn get_applied_snapshot_status(
        &mut self,
    ) -> Result<Option<AppliedSnapshotStatus>, sqlx::Error> {
        let record = sqlx::query!(
            "SELECT l1_batch_number, is_finished, last_finished_chunk_id \
            FROM applied_snapshot_status",
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(record.map(|r| AppliedSnapshotStatus {
            l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
            is_finished: r.is_finished,
            last_finished_chunk_id: r.last_finished_chunk_id.map(|v| v as u64),
        }))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{snapshots::AppliedSnapshotStatus, L1BatchNumber};

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
            is_finished: false,
            last_finished_chunk_id: None,
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
            is_finished: true,
            last_finished_chunk_id: Some(2345),
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
