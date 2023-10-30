use crate::StorageProcessor;
use sqlx::types::chrono::{DateTime, Utc};
use zksync_types::snapshots::{AllSnapshots, SnapshotBasicMetadata, SnapshotsWithFiles};
use zksync_types::{L1BatchNumber, MiniblockNumber};

#[derive(Debug)]
pub struct SnapshotsDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl SnapshotsDal<'_, '_> {
    pub async fn add_snapshot(
        &mut self,
        l1_batch_number: L1BatchNumber,
        miniblock_number: MiniblockNumber,
        files: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO snapshots (l1_batch_number, miniblock_number, created_at, files) \
             VALUES ($1, $2, now(), $3)",
            l1_batch_number.0 as i32,
            miniblock_number.0 as i32,
            files
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_snapshots(&mut self) -> Result<AllSnapshots, sqlx::Error> {
        let records: Vec<SnapshotBasicMetadata> =
            sqlx::query!("SELECT l1_batch_number, miniblock_number, created_at FROM snapshots")
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|r| SnapshotBasicMetadata {
                    l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
                    miniblock_number: MiniblockNumber(r.miniblock_number as u32),
                    generated_at: DateTime::<Utc>::from_naive_utc_and_offset(r.created_at, Utc),
                })
                .collect();
        Ok(AllSnapshots { snapshots: records })
    }

    pub async fn get_snapshot(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<SnapshotsWithFiles>, sqlx::Error> {
        let record = sqlx::query!(
            "SELECT l1_batch_number, miniblock_number, created_at, files \
            FROM snapshots WHERE l1_batch_number = $1",
            l1_batch_number.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(record.map(|r| SnapshotsWithFiles {
            metadata: SnapshotBasicMetadata {
                l1_batch_number: L1BatchNumber(r.l1_batch_number as u32),
                miniblock_number: MiniblockNumber(r.miniblock_number as u32),
                generated_at: DateTime::<Utc>::from_naive_utc_and_offset(r.created_at, Utc),
            },
            storage_logs_files: r.files,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::ConnectionPool;
    use db_test_macro::db_test;
    use zksync_types::{L1BatchNumber, MiniblockNumber};

    #[db_test(dal_crate)]
    async fn adding_snapshot(pool: ConnectionPool) {
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        let miniblock_number = MiniblockNumber(200);
        dal.add_snapshot(l1_batch_number, miniblock_number, &[])
            .await
            .expect("Failed to add snapshot");

        let snapshot = dal
            .get_snapshot(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot");
        assert!(snapshot.is_some());
        assert_eq!(
            snapshot.unwrap().metadata.l1_batch_number,
            l1_batch_number as L1BatchNumber
        );
    }

    #[db_test(dal_crate)]
    async fn adding_files(pool: ConnectionPool) {
        let mut conn = pool.access_storage().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        let miniblock_number = MiniblockNumber(200);
        dal.add_snapshot(
            l1_batch_number,
            miniblock_number,
            &["test_file1.bin".to_string(), "test_file2.bin".to_string()],
        )
        .await
        .expect("Failed to add snapshot");

        let snapshot = dal
            .get_snapshot(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot");
        assert!(snapshot.is_some());
        let files = &snapshot.unwrap().storage_logs_files;
        assert!(files.contains(&"test_file1.bin".to_string()));
        assert!(files.contains(&"test_file2.bin".to_string()));
    }
}
