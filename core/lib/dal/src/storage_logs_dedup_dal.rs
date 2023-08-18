use crate::StorageProcessor;
use sqlx::types::chrono::Utc;
use std::collections::HashSet;
use zksync_types::{
    zk_evm::aux_structures::LogQuery, AccountTreeId, Address, L1BatchNumber, StorageKey, H256,
};
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

        let last_index = self
            .max_set_enumeration_index()
            .await
            .map(|(last_index, _)| last_index)
            .unwrap_or(0);
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

    pub async fn max_set_enumeration_index(&mut self) -> Option<(u64, L1BatchNumber)> {
        sqlx::query!(
            "SELECT index, l1_batch_number FROM initial_writes \
            WHERE index IS NOT NULL \
            ORDER BY index DESC LIMIT 1",
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| {
            (
                row.index.unwrap() as u64,
                L1BatchNumber(row.l1_batch_number as u32),
            )
        })
    }

    pub async fn initial_writes_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Vec<(H256, Option<u64>)> {
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
        .map(|row| {
            (
                H256::from_slice(&row.hashed_key),
                row.index.map(|i| i as u64),
            )
        })
        .collect()
    }

    pub async fn set_indices_for_initial_writes(&mut self, indexed_keys: &[(H256, u64)]) {
        let (hashed_keys, indices): (Vec<_>, Vec<_>) = indexed_keys
            .iter()
            .map(|(hashed_key, index)| (hashed_key.as_bytes(), *index as i64))
            .unzip();
        sqlx::query!(
            "UPDATE initial_writes \
                SET index = data_table.index \
            FROM ( \
                SELECT UNNEST($1::bytea[]) as hashed_key, \
                    UNNEST($2::bigint[]) as index \
            ) as data_table \
            WHERE initial_writes.hashed_key = data_table.hashed_key",
            &hashed_keys as &[&[u8]],
            &indices,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
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

    // Used only for tests.
    pub async fn reset_indices(&mut self) {
        sqlx::query!(
            "UPDATE initial_writes \
            SET index = NULL",
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}
