use std::collections::HashSet;

use sqlx::types::chrono::Utc;
use zksync_types::{
    snapshots::SnapshotStorageLog, zk_evm_types::LogQuery, AccountTreeId, Address, L1BatchNumber,
    StorageKey, H256,
};
use zksync_utils::u256_to_h256;

pub use crate::models::storage_log::DbInitialWrite;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct StorageLogsDedupDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl StorageLogsDedupDal<'_, '_> {
    pub async fn insert_protective_reads(
        &mut self,
        l1_batch_number: L1BatchNumber,
        read_logs: &[LogQuery],
    ) -> sqlx::Result<()> {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY protective_reads (l1_batch_number, address, key, created_at, updated_at) \
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await?;

        let mut bytes: Vec<u8> = Vec::new();
        let now = Utc::now().naive_utc().to_string();
        for log in read_logs.iter() {
            let address_str = format!("\\\\x{}", hex::encode(log.address.0));
            let key_str = format!("\\\\x{}", hex::encode(u256_to_h256(log.key).0));
            let row = format!(
                "{}|{}|{}|{}|{}\n",
                l1_batch_number, address_str, key_str, now, now
            );
            bytes.extend_from_slice(row.as_bytes());
        }
        copy.send(bytes).await?;
        copy.finish().await?;
        Ok(())
    }

    /// Insert initial writes and assigns indices to them.
    /// Assumes indices are already assigned for all saved initial_writes, so must be called only after the migration.
    pub async fn insert_initial_writes_from_snapshot(
        &mut self,
        snapshot_storage_logs: &[SnapshotStorageLog],
    ) -> sqlx::Result<()> {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY initial_writes (hashed_key, index, l1_batch_number, created_at, updated_at) \
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await?;

        let mut bytes: Vec<u8> = Vec::new();
        let now = Utc::now().naive_utc().to_string();
        for log in snapshot_storage_logs.iter() {
            let row = format!(
                "\\\\x{:x}|{}|{}|{}|{}\n",
                log.key.hashed_key(),
                log.enumeration_index,
                log.l1_batch_number_of_initial_write,
                now,
                now,
            );
            bytes.extend_from_slice(row.as_bytes());
        }
        copy.send(bytes).await?;
        copy.finish().await?;

        Ok(())
    }

    pub async fn insert_initial_writes(
        &mut self,
        l1_batch_number: L1BatchNumber,
        written_storage_keys: &[StorageKey],
    ) -> sqlx::Result<()> {
        let hashed_keys: Vec<_> = written_storage_keys
            .iter()
            .map(|key| StorageKey::raw_hashed_key(key.address(), key.key()).to_vec())
            .collect();

        let last_index = self.max_enumeration_index().await?.unwrap_or(0);
        let indices: Vec<_> = ((last_index + 1)..=(last_index + hashed_keys.len() as u64))
            .map(|x| x as i64)
            .collect();

        sqlx::query!(
            r#"
            INSERT INTO
                initial_writes (hashed_key, INDEX, l1_batch_number, created_at, updated_at)
            SELECT
                u.hashed_key,
                u.index,
                $3,
                NOW(),
                NOW()
            FROM
                UNNEST($1::bytea[], $2::BIGINT[]) AS u (hashed_key, INDEX)
            "#,
            &hashed_keys,
            &indices,
            i64::from(l1_batch_number.0)
        )
        .execute(self.storage.conn())
        .await?;

        Ok(())
    }

    pub async fn get_protective_reads_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashSet<StorageKey> {
        sqlx::query!(
            r#"
            SELECT
                address,
                key
            FROM
                protective_reads
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            StorageKey::new(
                AccountTreeId::new(Address::from_slice(&row.address)),
                H256::from_slice(&row.key),
            )
        })
        .collect()
    }

    async fn max_enumeration_index(&mut self) -> sqlx::Result<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                MAX(INDEX) AS "max?"
            FROM
                initial_writes
            "#,
        )
        .fetch_one(self.storage.conn())
        .await?
        .max
        .map(|max| max as u64))
    }

    pub async fn initial_writes_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Vec<(H256, u64)>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                hashed_key,
                INDEX
            FROM
                initial_writes
            WHERE
                l1_batch_number = $1
            ORDER BY
                INDEX
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| (H256::from_slice(&row.hashed_key), row.index as u64))
        .collect())
    }

    pub async fn get_enumeration_index_for_key(
        &mut self,
        hashed_key: H256,
    ) -> sqlx::Result<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                INDEX
            FROM
                initial_writes
            WHERE
                hashed_key = $1
            "#,
            hashed_key.as_bytes()
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.index as u64))
    }

    /// Returns `hashed_keys` that are both present in the input and in `initial_writes` table.
    pub async fn filter_written_slots(
        &mut self,
        hashed_keys: &[H256],
    ) -> sqlx::Result<HashSet<H256>> {
        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();
        Ok(sqlx::query!(
            r#"
            SELECT
                hashed_key
            FROM
                initial_writes
            WHERE
                hashed_key = ANY ($1)
            "#,
            &hashed_keys as &[&[u8]],
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| H256::from_slice(&row.hashed_key))
        .collect())
    }

    /// Retrieves all initial write entries for testing purposes.
    pub async fn dump_all_initial_writes_for_tests(&mut self) -> Vec<DbInitialWrite> {
        let rows = sqlx::query!(
            r#"
            SELECT
                hashed_key,
                l1_batch_number,
                INDEX
            FROM
                initial_writes
            "#
        )
        .fetch_all(self.storage.conn())
        .await
        .expect("get_all_initial_writes_for_tests");

        rows.into_iter()
            .map(|row| DbInitialWrite {
                hashed_key: H256::from_slice(&row.hashed_key),
                l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
                index: row.index as u64,
            })
            .collect()
    }
}
