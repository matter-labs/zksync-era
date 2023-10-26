use crate::StorageProcessor;
use sqlx::types::chrono::Utc;
use std::collections::HashSet;
use zksync_types::{AccountTreeId, Address, L1BatchNumber, LogQuery, StorageKey, H256};
use zksync_utils::u256_to_h256;

#[derive(Debug)]
pub struct StorageLogsDedupDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl StorageLogsDedupDal<'_, '_> {
    pub async fn insert_protective_reads(
        &mut self,
        l1_batch_number: L1BatchNumber,
        read_logs: &[LogQuery],
    ) {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY protective_reads (l1_batch_number, address, key, created_at, updated_at) \
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

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
        copy.send(bytes).await.unwrap();
        copy.finish().await.unwrap();
    }

    /// Insert initial writes and assigns indices to them.
    /// Assumes indices are already assigned for all saved initial_writes, so must be called only after the migration.
    pub async fn insert_initial_writes(
        &mut self,
        l1_batch_number: L1BatchNumber,
        written_storage_keys: &[StorageKey],
    ) {
        let hashed_keys: Vec<_> = written_storage_keys
            .iter()
            .map(|key| StorageKey::raw_hashed_key(key.address(), key.key()).to_vec())
            .collect();

        let last_index = self.max_enumeration_index().await.unwrap_or(0);
        let indices: Vec<_> = ((last_index + 1)..=(last_index + hashed_keys.len() as u64))
            .map(|x| x as i64)
            .collect();

        sqlx::query!(
            "INSERT INTO initial_writes (hashed_key, index, l1_batch_number, created_at, updated_at) \
            SELECT u.hashed_key, u.index, $3, now(), now() \
            FROM UNNEST($1::bytea[], $2::bigint[]) AS u(hashed_key, index)",
            &hashed_keys,
            &indices,
            l1_batch_number.0 as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_protective_reads_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashSet<StorageKey> {
        sqlx::query!(
            "SELECT address, key FROM protective_reads WHERE l1_batch_number = $1",
            l1_batch_number.0 as i64
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

    pub async fn max_enumeration_index(&mut self) -> Option<u64> {
        sqlx::query!("SELECT MAX(index) as \"max?\" FROM initial_writes",)
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .max
            .map(|max| max as u64)
    }

    pub async fn initial_writes_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Vec<(H256, u64)> {
        sqlx::query!(
            "SELECT hashed_key, index FROM initial_writes \
            WHERE l1_batch_number = $1 \
            ORDER BY index",
            l1_batch_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (H256::from_slice(&row.hashed_key), row.index as u64))
        .collect()
    }

    pub async fn get_enumeration_index_for_key(&mut self, key: StorageKey) -> Option<u64> {
        sqlx::query!(
            "SELECT index \
             FROM initial_writes \
             WHERE hashed_key = $1",
            key.hashed_key().0.to_vec()
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.index as u64)
    }

    /// Returns `hashed_keys` that are both present in the input and in `initial_writes` table.
    pub async fn filter_written_slots(&mut self, hashed_keys: &[H256]) -> HashSet<H256> {
        let hashed_keys: Vec<_> = hashed_keys.iter().map(H256::as_bytes).collect();
        sqlx::query!(
            "SELECT hashed_key FROM initial_writes \
            WHERE hashed_key = ANY($1)",
            &hashed_keys as &[&[u8]],
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| H256::from_slice(&row.hashed_key))
        .collect()
    }
}
