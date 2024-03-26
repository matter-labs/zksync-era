use zksync_db_connection::{connection::Connection, instrument::InstrumentExt};
use zksync_types::{
    snapshots::{AllSnapshots, SnapshotMetadata, SnapshotVersion},
    L1BatchNumber,
};

use crate::Core;

#[derive(Debug, sqlx::FromRow)]
struct StorageSnapshotMetadata {
    version: i32,
    l1_batch_number: i64,
    storage_logs_filepaths: Vec<String>,
    factory_deps_filepath: String,
}

impl TryFrom<StorageSnapshotMetadata> for SnapshotMetadata {
    type Error = sqlx::Error;

    fn try_from(row: StorageSnapshotMetadata) -> Result<Self, Self::Error> {
        let int_version = u16::try_from(row.version).map_err(|err| sqlx::Error::ColumnDecode {
            index: "version".to_owned(),
            source: err.into(),
        })?;
        let version =
            SnapshotVersion::try_from(int_version).map_err(|err| sqlx::Error::ColumnDecode {
                index: "version".to_owned(),
                source: err.into(),
            })?;

        Ok(Self {
            version,
            l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
            storage_logs_filepaths: row
                .storage_logs_filepaths
                .into_iter()
                .map(|path| (!path.is_empty()).then_some(path))
                .collect(),
            factory_deps_filepath: row.factory_deps_filepath,
        })
    }
}

#[derive(Debug)]
pub struct SnapshotsDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl SnapshotsDal<'_, '_> {
    pub async fn add_snapshot(
        &mut self,
        version: SnapshotVersion,
        l1_batch_number: L1BatchNumber,
        storage_logs_chunk_count: u64,
        factory_deps_filepaths: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                snapshots (
                    VERSION,
                    l1_batch_number,
                    storage_logs_filepaths,
                    factory_deps_filepath,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, ARRAY_FILL(''::TEXT, ARRAY[$3::INTEGER]), $4, NOW(), NOW())
            "#,
            version as i32,
            l1_batch_number.0 as i32,
            storage_logs_chunk_count as i32,
            factory_deps_filepaths,
        )
        .instrument("add_snapshot")
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn add_storage_logs_filepath_for_snapshot(
        &mut self,
        l1_batch_number: L1BatchNumber,
        chunk_id: u64,
        storage_logs_filepath: &str,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE snapshots
            SET
                storage_logs_filepaths[$2] = $3,
                updated_at = NOW()
            WHERE
                l1_batch_number = $1
            "#,
            l1_batch_number.0 as i32,
            chunk_id as i32 + 1,
            storage_logs_filepath,
        )
        .execute(self.storage.conn())
        .await?;

        Ok(())
    }

    pub async fn get_all_complete_snapshots(&mut self) -> sqlx::Result<AllSnapshots> {
        let rows = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                snapshots
            WHERE
                NOT (''::TEXT = ANY (storage_logs_filepaths))
            ORDER BY
                l1_batch_number DESC
            "#
        )
        .instrument("get_all_complete_snapshots")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        let snapshots_l1_batch_numbers = rows
            .into_iter()
            .map(|row| L1BatchNumber(row.l1_batch_number as u32))
            .collect();

        Ok(AllSnapshots {
            snapshots_l1_batch_numbers,
        })
    }

    pub async fn get_newest_snapshot_metadata(&mut self) -> sqlx::Result<Option<SnapshotMetadata>> {
        let row = sqlx::query_as!(
            StorageSnapshotMetadata,
            r#"
            SELECT
                VERSION,
                l1_batch_number,
                factory_deps_filepath,
                storage_logs_filepaths
            FROM
                snapshots
            ORDER BY
                l1_batch_number DESC
            LIMIT
                1
            "#
        )
        .instrument("get_newest_snapshot_metadata")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        row.map(TryFrom::try_from).transpose()
    }

    pub async fn get_snapshot_metadata(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<SnapshotMetadata>> {
        let row = sqlx::query_as!(
            StorageSnapshotMetadata,
            r#"
            SELECT
                VERSION,
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
        .fetch_optional(self.storage)
        .await?;

        row.map(TryFrom::try_from).transpose()
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{snapshots::SnapshotVersion, L1BatchNumber};

    use crate::{ConnectionPool, Core, CoreDal};

    #[tokio::test]
    async fn adding_snapshot() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(
            SnapshotVersion::Version0,
            l1_batch_number,
            2,
            "gs:///bucket/factory_deps.bin",
        )
        .await
        .expect("Failed to add snapshot");

        let snapshots = dal
            .get_all_complete_snapshots()
            .await
            .expect("Failed to retrieve snapshots");
        assert_eq!(snapshots.snapshots_l1_batch_numbers, []);

        for i in 0..2 {
            dal.add_storage_logs_filepath_for_snapshot(
                l1_batch_number,
                i,
                "gs:///bucket/chunk.bin",
            )
            .await
            .unwrap();
        }

        let snapshots = dal
            .get_all_complete_snapshots()
            .await
            .expect("Failed to retrieve snapshots");
        assert_eq!(snapshots.snapshots_l1_batch_numbers, [l1_batch_number]);

        let snapshot_metadata = dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot")
            .unwrap();
        assert_eq!(snapshot_metadata.l1_batch_number, l1_batch_number);
    }

    #[tokio::test]
    async fn adding_files() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let mut dal = conn.snapshots_dal();
        let l1_batch_number = L1BatchNumber(100);
        dal.add_snapshot(
            SnapshotVersion::Version0,
            l1_batch_number,
            2,
            "gs:///bucket/factory_deps.bin",
        )
        .await
        .expect("Failed to add snapshot");

        let storage_log_filepaths = ["gs:///bucket/test_file1.bin", "gs:///bucket/test_file2.bin"];
        dal.add_storage_logs_filepath_for_snapshot(l1_batch_number, 1, storage_log_filepaths[1])
            .await
            .unwrap();

        let files = dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot")
            .unwrap()
            .storage_logs_filepaths;
        assert_eq!(
            files,
            [None, Some("gs:///bucket/test_file2.bin".to_string())]
        );

        dal.add_storage_logs_filepath_for_snapshot(l1_batch_number, 0, storage_log_filepaths[0])
            .await
            .unwrap();

        let files = dal
            .get_snapshot_metadata(l1_batch_number)
            .await
            .expect("Failed to retrieve snapshot")
            .unwrap()
            .storage_logs_filepaths;
        assert_eq!(
            files,
            [
                Some("gs:///bucket/test_file1.bin".to_string()),
                Some("gs:///bucket/test_file2.bin".to_string())
            ]
        );
    }
}
