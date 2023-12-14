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
        storage_logs_chunk_count: u64,
        factory_deps_filepaths: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            "INSERT INTO snapshots ( \
                l1_batch_number, storage_logs_chunk_count, storage_logs_filepaths, factory_deps_filepath, \
                created_at, updated_at \
            ) \
            VALUES ($1, $2, ARRAY[]::text[], $3, NOW(), NOW())",
            l1_batch_number.0 as i32,
            storage_logs_chunk_count as i64,
            factory_deps_filepaths,
        )
        .instrument("add_snapshot")
        .report_latency()
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn add_storage_logs_filepath_for_snapshot(
        &mut self,
        l1_batch_number: L1BatchNumber,
        storage_logs_filepath: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE snapshots SET \
                storage_logs_filepaths = storage_logs_filepaths || $2::text, \
                updated_at = NOW() \
            WHERE l1_batch_number = $1",
            l1_batch_number.0 as i32,
            storage_logs_filepath,
        )
        .execute(self.storage.conn())
        .await?;

        Ok(())
    }

    pub async fn get_all_snapshots(&mut self) -> sqlx::Result<AllSnapshots> {
        let records =
            sqlx::query!("SELECT l1_batch_number FROM snapshots ORDER BY l1_batch_number")
                .instrument("get_all_snapshots")
                .report_latency()
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|row| L1BatchNumber(row.l1_batch_number as u32))
                .collect();

        Ok(AllSnapshots {
            snapshots_l1_batch_numbers: records,
        })
    }

    pub async fn get_newest_snapshot_metadata(&mut self) -> sqlx::Result<Option<SnapshotMetadata>> {
        let row = sqlx::query!(
            "SELECT l1_batch_number, storage_logs_chunk_count, factory_deps_filepath, storage_logs_filepaths \
            FROM snapshots \
            ORDER BY l1_batch_number DESC LIMIT 1"
        )
        .instrument("get_newest_snapshot_metadata")
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(row.map(|row| SnapshotMetadata {
            l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
            storage_logs_chunk_count: row.storage_logs_chunk_count as u64,
            factory_deps_filepath: row.factory_deps_filepath,
            storage_logs_filepaths: row.storage_logs_filepaths,
        }))
    }

    pub async fn get_snapshot_metadata(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<SnapshotMetadata>> {
        let row = sqlx::query!(
            "SELECT l1_batch_number, storage_logs_chunk_count, factory_deps_filepath, storage_logs_filepaths \
            FROM snapshots \
            WHERE l1_batch_number = $1",
            l1_batch_number.0 as i32
        )
        .instrument("get_snapshot_metadata")
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(row.map(|row| SnapshotMetadata {
            l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
            storage_logs_chunk_count: row.storage_logs_chunk_count as u64,
            factory_deps_filepath: row.factory_deps_filepath,
            storage_logs_filepaths: row.storage_logs_filepaths,
        }))
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
        dal.add_snapshot(l1_batch_number, 100, "gs:///bucket/factory_deps.bin")
            .await
            .expect("Failed to add snapshot");

        let snapshots = dal
            .get_all_snapshots()
            .await
            .expect("Failed to retrieve snapshots");
        assert_eq!(1, snapshots.snapshots_l1_batch_numbers.len());
        assert_eq!(snapshots.snapshots_l1_batch_numbers[0], l1_batch_number);

        let snapshot_metadata = dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot")
            .unwrap();
        assert_eq!(snapshot_metadata.l1_batch_number, l1_batch_number);
    }

    #[tokio::test]
    async fn adding_files() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(l1_batch_number, 100, "gs:///bucket/factory_deps.bin")
            .await
            .expect("Failed to add snapshot");

        let storage_log_filepaths = ["gs:///bucket/test_file1.bin", "gs:///bucket/test_file2.bin"];
        for storage_log_filepath in storage_log_filepaths {
            dal.add_storage_logs_filepath_for_snapshot(l1_batch_number, storage_log_filepath)
                .await
                .unwrap();
        }

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
