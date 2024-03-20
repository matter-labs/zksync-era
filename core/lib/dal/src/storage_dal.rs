use std::collections::HashMap;

use itertools::Itertools;
use zksync_db_connection::connection::Connection;
use zksync_types::{StorageKey, StorageLog, StorageValue, H256};

use crate::Core;

#[derive(Debug)]
pub struct StorageDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[deprecated(note = "Soft-removed in favor of `storage_logs`; don't use")]
impl StorageDal<'_, '_> {
    /// Applies the specified storage logs for a miniblock. Returns the map of unique storage updates.
    pub async fn apply_storage_logs(
        &mut self,
        updates: &[(H256, Vec<StorageLog>)],
    ) -> HashMap<StorageKey, (H256, StorageValue)> {
        let unique_updates: HashMap<_, _> = updates
            .iter()
            .flat_map(|(tx_hash, storage_logs)| {
                storage_logs
                    .iter()
                    .map(move |log| (log.key, (*tx_hash, log.value)))
            })
            .collect();

        let query_parts = unique_updates.iter().map(|(key, (tx_hash, value))| {
            (
                key.hashed_key().0.to_vec(),
                key.address().0.as_slice(),
                key.key().0.as_slice(),
                value.as_bytes(),
                tx_hash.0.as_slice(),
            )
        });
        let (hashed_keys, addresses, keys, values, tx_hashes): (
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
        ) = query_parts.multiunzip();

        // Copy from stdin can't be used here because of `ON CONFLICT`.
        sqlx::query!(
            r#"
            INSERT INTO
                storage (hashed_key, address, key, value, tx_hash, created_at, updated_at)
            SELECT
                u.hashed_key,
                u.address,
                u.key,
                u.value,
                u.tx_hash,
                NOW(),
                NOW()
            FROM
                UNNEST($1::bytea[], $2::bytea[], $3::bytea[], $4::bytea[], $5::bytea[]) AS u (hashed_key, address, key, value, tx_hash)
            ON CONFLICT (hashed_key) DO
            UPDATE
            SET
                tx_hash = excluded.tx_hash,
                value = excluded.value,
                updated_at = NOW()
            "#,
            &hashed_keys,
            &addresses as &[&[u8]],
            &keys as &[&[u8]],
            &values as &[&[u8]],
            &tx_hashes as &[&[u8]],
        )
        .execute(self.storage.conn())
        .await
        .unwrap();

        unique_updates
    }

    #[cfg(test)]
    pub(crate) async fn get_by_key(&mut self, key: &StorageKey) -> sqlx::Result<Option<H256>> {
        use sqlx::Row as _;

        let row = sqlx::query("SELECT value FROM storage WHERE hashed_key = $1::bytea")
            .bind(key.hashed_key().as_bytes())
            .fetch_optional(self.storage.conn())
            .await?;
        Ok(row.map(|row| {
            let raw_value: Vec<u8> = row.get("value");
            H256::from_slice(&raw_value)
        }))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{AccountTreeId, Address};

    use super::*;
    use crate::{ConnectionPool, Core, CoreDal};

    #[allow(deprecated)]
    #[tokio::test]
    async fn applying_storage_logs() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let account = AccountTreeId::new(Address::repeat_byte(1));
        let first_key = StorageKey::new(account, H256::zero());
        let second_key = StorageKey::new(account, H256::from_low_u64_be(1));
        let storage_logs = vec![
            StorageLog::new_write_log(first_key, H256::repeat_byte(1)),
            StorageLog::new_write_log(second_key, H256::repeat_byte(2)),
        ];
        let updates = [(H256::repeat_byte(1), storage_logs)];
        conn.storage_dal().apply_storage_logs(&updates).await;

        let first_value = conn.storage_dal().get_by_key(&first_key).await.unwrap();
        assert_eq!(first_value, Some(H256::repeat_byte(1)));
        let second_value = conn.storage_dal().get_by_key(&second_key).await.unwrap();
        assert_eq!(second_value, Some(H256::repeat_byte(2)));
    }
}
