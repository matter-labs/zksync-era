use crate::StorageProcessor;
use zksync_types::snapshots::AppliedSnapshotStatus;
use zksync_types::L1BatchNumber;

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
