use std::collections::HashSet;

use sqlx::types::chrono::Utc;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{CopyStatement, InstrumentExt},
};
use zksync_types::{
    snapshots::SnapshotStorageLog, AccountTreeId, Address, L1BatchNumber, StorageKey, StorageLog,
    H256,
};

pub use crate::models::storage_log::DbInitialWrite;
use crate::Core;

#[derive(Debug)]
pub struct StorageLogsDedupDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl StorageLogsDedupDal<'_, '_> {
    pub async fn insert_protective_reads(
        &mut self,
        l1_batch_number: L1BatchNumber,
        read_logs: &[StorageLog],
    ) -> DalResult<()> {
        let read_logs_len = read_logs.len();
        let copy = CopyStatement::new(
            "COPY protective_reads (l1_batch_number, address, key, created_at, updated_at) \
             FROM STDIN WITH (DELIMITER '|')",
        )
        .instrument("insert_protective_reads")
        .with_arg("l1_batch_number", &l1_batch_number)
        .with_arg("read_logs.len", &read_logs_len)
        .start(self.storage)
        .await?;

        let mut bytes: Vec<u8> = Vec::new();
        let now = Utc::now().naive_utc().to_string();
        for log in read_logs.iter() {
            let address_str = format!("\\\\x{}", hex::encode(log.key.address()));
            let key_str = format!("\\\\x{}", hex::encode(log.key.key()));
            let row = format!(
                "{}|{}|{}|{}|{}\n",
                l1_batch_number, address_str, key_str, now, now
            );
            bytes.extend_from_slice(row.as_bytes());
        }
        copy.send(&bytes).await
    }

    /// Insert initial writes and assigns indices to them.
    /// Assumes indices are already assigned for all saved initial_writes, so must be called only after the migration.
    pub async fn insert_initial_writes_from_snapshot(
        &mut self,
        snapshot_storage_logs: &[SnapshotStorageLog],
    ) -> DalResult<()> {
        let storage_logs_len = snapshot_storage_logs.len();
        let copy = CopyStatement::new(
            "COPY initial_writes (hashed_key, index, l1_batch_number, created_at, updated_at) \
             FROM STDIN WITH (DELIMITER '|')",
        )
        .instrument("insert_initial_writes_from_snapshot")
        .with_arg("storage_logs.len", &storage_logs_len)
        .start(self.storage)
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
        copy.send(&bytes).await
    }

    pub async fn insert_initial_writes(
        &mut self,
        l1_batch_number: L1BatchNumber,
        written_storage_keys: &[StorageKey],
    ) -> DalResult<()> {
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
        .instrument("insert_initial_writes")
        .with_arg("l1_batch_number", &l1_batch_number)
        .with_arg("hashed_keys.len", &hashed_keys.len())
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_protective_reads_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<HashSet<StorageKey>> {
        let rows = sqlx::query!(
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
        .instrument("get_protective_reads_for_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                StorageKey::new(
                    AccountTreeId::new(Address::from_slice(&row.address)),
                    H256::from_slice(&row.key),
                )
            })
            .collect())
    }

    async fn max_enumeration_index(&mut self) -> DalResult<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                MAX(INDEX) AS "max?"
            FROM
                initial_writes
            "#,
        )
        .instrument("max_enumeration_index")
        .fetch_one(self.storage)
        .await?
        .max
        .map(|max| max as u64))
    }

    /// Returns the max enumeration index by the provided L1 batch number.
    pub async fn max_enumeration_index_by_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                MAX(INDEX) AS "max?"
            FROM
                initial_writes
            WHERE
                l1_batch_number = (
                    SELECT
                        MAX(l1_batch_number) AS "max?"
                    FROM
                        initial_writes
                    WHERE
                        l1_batch_number <= $1
                )
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("max_enumeration_index_by_l1_batch")
        .fetch_one(self.storage)
        .await?
        .max
        .map(|max| max as u64))
    }

    pub async fn initial_writes_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Vec<(H256, u64)>> {
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
        .instrument("initial_writes_for_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| (H256::from_slice(&row.hashed_key), row.index as u64))
        .collect())
    }

    pub async fn get_enumeration_index_for_key(
        &mut self,
        hashed_key: H256,
    ) -> DalResult<Option<u64>> {
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
        .instrument("get_enumeration_index_for_key")
        .with_arg("hashed_key", &hashed_key)
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.index as u64))
    }

    pub async fn get_enumeration_index_in_l1_batch(
        &mut self,
        hashed_key: H256,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                INDEX
            FROM
                initial_writes
            WHERE
                hashed_key = $1
                AND l1_batch_number <= $2
            "#,
            hashed_key.as_bytes(),
            l1_batch_number.0 as i32,
        )
        .instrument("get_enumeration_index_in_l1_batch")
        .with_arg("hashed_key", &hashed_key)
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.index as u64))
    }

    /// Returns `hashed_keys` that are both present in the input and in `initial_writes` table.
    pub async fn filter_written_slots(&mut self, hashed_keys: &[H256]) -> DalResult<HashSet<H256>> {
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
        .instrument("filter_written_slots")
        .with_arg("hashed_keys.len", &hashed_keys.len())
        .fetch_all(self.storage)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionPool, CoreDal};

    #[tokio::test]
    async fn getting_max_enumeration_index_in_batch() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        let max_index = conn
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();
        assert_eq!(max_index, None);

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let initial_writes = [
            StorageKey::new(account, H256::zero()),
            StorageKey::new(account, H256::repeat_byte(1)),
        ];
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(0), &initial_writes)
            .await
            .unwrap();

        let max_index = conn
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();
        assert_eq!(max_index, Some(2));

        let initial_writes = [
            StorageKey::new(account, H256::repeat_byte(2)),
            StorageKey::new(account, H256::repeat_byte(3)),
        ];
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(1), &initial_writes)
            .await
            .unwrap();

        let max_index = conn
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();
        assert_eq!(max_index, Some(2));

        let max_index = conn
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();
        assert_eq!(max_index, Some(4));
    }
}
