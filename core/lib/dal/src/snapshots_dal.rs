use crate::StorageProcessor;
use sqlx::types::chrono::{DateTime, Utc};
use zksync_types::snapshots::{AllSnapshots, SnapshotMetadata};
use zksync_types::L1BatchNumber;

#[derive(Debug)]
pub struct SnapshotsDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotsDal<'_, '_> {
    pub async fn add_snapshot(
        &mut self,
        l1_batch_number: L1BatchNumber,
        files: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO snapshots (l1_batch_number, created_at, files) \
             VALUES ($1, now(), $2)",
            l1_batch_number.0 as i32,
            files
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_all_snapshots(&mut self) -> Result<AllSnapshots, sqlx::Error> {
        let records: Vec<SnapshotMetadata> =
            sqlx::query!("SELECT l1_batch_number, created_at FROM snapshots")
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|r| SnapshotMetadata {
                    l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
                    generated_at: DateTime::<Utc>::from_naive_utc_and_offset(r.created_at, Utc),
                })
                .collect();
        Ok(AllSnapshots { snapshots: records })
    }

    pub async fn get_snapshot_metadata(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotMetadata>, sqlx::Error> {
        let record: Option<SnapshotMetadata> = sqlx::query!(
            "SELECT l1_batch_number, created_at FROM snapshots WHERE l1_batch_number = $1",
            l1_batch_number.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|r| SnapshotMetadata {
            l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
            generated_at: DateTime::<Utc>::from_naive_utc_and_offset(r.created_at, Utc),
        });
        Ok(record)
    }

    pub async fn get_snapshot_files(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<Vec<String>>, sqlx::Error> {
        let record = sqlx::query!(
            "SELECT l1_batch_number, created_at, files \
            FROM snapshots WHERE l1_batch_number = $1",
            l1_batch_number.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(record.map(|r| r.files))
    }
}

#[cfg(test)]
mod tests {
    use crate::ConnectionPool;
    use db_test_macro::db_test;
    use zksync_types::L1BatchNumber;

    #[db_test(dal_crate)]
    async fn adding_snapshot(pool: ConnectionPool) {
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(l1_batch_number, &[])
            .await
            .expect("Failed to add snapshot");

        let snapshots = dal
            .get_all_snapshots()
            .await
            .expect("Failed to retrieve snapshots");
        assert_eq!(1, snapshots.snapshots.len());
        assert_eq!(
            snapshots.snapshots[0].l1_batch_number,
            l1_batch_number as L1BatchNumber
        );

        let snapshot_metadata = dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot")
            .unwrap();
        assert_eq!(
            snapshot_metadata.l1_batch_number,
            l1_batch_number as L1BatchNumber
        );
    }

    #[db_test(dal_crate)]
    async fn adding_files(pool: ConnectionPool) {
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(
            l1_batch_number,
            &[
                "gs:///bucket/test_file1.bin".to_string(),
                "gs:///bucket/test_file2.bin".to_string(),
            ],
        )
        .await
        .expect("Failed to add snapshot");

        let files = dal
            .get_snapshot_files(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot");
        assert!(files.is_some());
        let files = files.unwrap();
        assert!(files.contains(&"gs:///bucket/test_file1.bin".to_string()));
        assert!(files.contains(&"gs:///bucket/test_file2.bin".to_string()));
    }
}
