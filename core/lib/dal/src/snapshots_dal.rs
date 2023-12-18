use zksync_types::{
    snapshots::{AllSnapshots, SnapshotMetadata},
    L1BatchNumber,
};

use crate::{instrument::InstrumentExt, StorageProcessor};

#[derive(Debug)]
pub struct SnapshotsDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotsDal<'_, '_> {
    pub async fn add_snapshot(
        &mut self,
        l1_batch_number: L1BatchNumber,
        storage_logs_filepaths: &[String],
        factory_deps_filepaths: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO
                snapshots (
                    l1_batch_number,
                    storage_logs_filepaths,
                    factory_deps_filepath,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3, NOW(), NOW())
            "#,
            l1_batch_number.0 as i32,
            storage_logs_filepaths,
            factory_deps_filepaths,
        )
        .instrument("add_snapshot")
        .report_latency()
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_all_snapshots(&mut self) -> Result<AllSnapshots, sqlx::Error> {
        let records: Vec<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                factory_deps_filepath,
                storage_logs_filepaths
            FROM
                snapshots
            "#
        )
        .instrument("get_all_snapshots")
        .report_latency()
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|r| L1BatchNumber(r.l1_batch_number as u32))
        .collect();
        Ok(AllSnapshots {
            snapshots_l1_batch_numbers: records,
        })
    }

    pub async fn get_snapshot_metadata(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotMetadata>, sqlx::Error> {
        let record: Option<SnapshotMetadata> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                factory_deps_filepath,
                storage_logs_filepaths
            FROM
                snapshots
            WHERE
                l1_batch_number = $1
            "#,
            l1_batch_number.0 as i32
        )
        .instrument("get_snapshot_metadata")
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await?
        .map(|r| SnapshotMetadata {
            l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
            factory_deps_filepath: r.factory_deps_filepath,
            storage_logs_filepaths: r.storage_logs_filepaths,
        });
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::L1BatchNumber;

    use crate::ConnectionPool;

    #[tokio::test]
    async fn adding_snapshot() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(l1_batch_number, &[], "gs:///bucket/factory_deps.bin")
            .await
            .expect("Failed to add snapshot");

        let snapshots = dal
            .get_all_snapshots()
            .await
            .expect("Failed to retrieve snapshots");
        assert_eq!(1, snapshots.snapshots_l1_batch_numbers.len());
        assert_eq!(
            snapshots.snapshots_l1_batch_numbers[0],
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

    #[tokio::test]
    async fn adding_files() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(
            l1_batch_number,
            &[
                "gs:///bucket/test_file1.bin".to_string(),
                "gs:///bucket/test_file2.bin".to_string(),
            ],
            "gs:///bucket/factory_deps.bin",
        )
        .await
        .expect("Failed to add snapshot");

        let files = dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot")
            .unwrap()
            .storage_logs_filepaths;
        assert!(files.contains(&"gs:///bucket/test_file1.bin".to_string()));
        assert!(files.contains(&"gs:///bucket/test_file2.bin".to_string()));
    }
}
