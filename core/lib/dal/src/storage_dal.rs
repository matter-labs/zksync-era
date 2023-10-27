use itertools::Itertools;

use std::collections::{HashMap, HashSet};

use zksync_contracts::{BaseSystemContracts, SystemContractCode};
use zksync_types::{MiniblockNumber, StorageKey, StorageLog, StorageValue, H256, U256};
use zksync_utils::{bytes_to_be_words, bytes_to_chunks};

use crate::{instrument::InstrumentExt, StorageProcessor};

#[derive(Debug)]
pub struct StorageDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl StorageDal<'_, '_> {
    /// Inserts factory dependencies for a miniblock. Factory deps are specified as a map of
    /// `(bytecode_hash, bytecode)` entries.
    pub async fn insert_factory_deps(
        &mut self,
        block_number: MiniblockNumber,
        factory_deps: &HashMap<H256, Vec<u8>>,
    ) {
        let (bytecode_hashes, bytecodes): (Vec<_>, Vec<_>) = factory_deps
            .iter()
            .map(|dep| (dep.0.as_bytes(), dep.1.as_slice()))
            .unzip();

        // Copy from stdin can't be used here because of 'ON CONFLICT'.
        sqlx::query!(
            "INSERT INTO factory_deps \
            (bytecode_hash, bytecode, miniblock_number, created_at, updated_at) \
            SELECT u.bytecode_hash, u.bytecode, $3, now(), now() \
                FROM UNNEST($1::bytea[], $2::bytea[]) \
                AS u(bytecode_hash, bytecode) \
            ON CONFLICT (bytecode_hash) DO NOTHING",
            &bytecode_hashes as &[&[u8]],
            &bytecodes as &[&[u8]],
            block_number.0 as i64,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    /// Returns bytecode for a factory dep with the specified bytecode `hash`.
    pub async fn get_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        sqlx::query!(
            "SELECT bytecode FROM factory_deps WHERE bytecode_hash = $1",
            hash.as_bytes(),
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.bytecode)
    }

    pub async fn get_base_system_contracts(
        &mut self,
        bootloader_hash: H256,
        default_aa_hash: H256,
    ) -> BaseSystemContracts {
        let bootloader_bytecode = self
            .get_factory_dep(bootloader_hash)
            .await
            .expect("Bootloader code should be present in the database");
        let bootloader_code = SystemContractCode {
            code: bytes_to_be_words(bootloader_bytecode),
            hash: bootloader_hash,
        };

        let default_aa_bytecode = self
            .get_factory_dep(default_aa_hash)
            .await
            .expect("Default account code should be present in the database");

        let default_aa_code = SystemContractCode {
            code: bytes_to_be_words(default_aa_bytecode),
            hash: default_aa_hash,
        };
        BaseSystemContracts {
            bootloader: bootloader_code,
            default_aa: default_aa_code,
        }
    }

    /// Returns bytecodes for factory deps with the specified `hashes`.
    pub async fn get_factory_deps(
        &mut self,
        hashes: &HashSet<H256>,
    ) -> HashMap<U256, Vec<[u8; 32]>> {
        let hashes_as_bytes: Vec<_> = hashes.iter().map(H256::as_bytes).collect();

        sqlx::query!(
            "SELECT bytecode, bytecode_hash FROM factory_deps WHERE bytecode_hash = ANY($1)",
            &hashes_as_bytes as &[&[u8]],
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                U256::from_big_endian(&row.bytecode_hash),
                bytes_to_chunks(&row.bytecode),
            )
        })
        .collect()
    }

    /// Returns bytecode hashes for factory deps from miniblocks with number strictly greater
    /// than `block_number`.
    pub async fn get_factory_deps_for_revert(
        &mut self,
        block_number: MiniblockNumber,
    ) -> Vec<H256> {
        sqlx::query!(
            "SELECT bytecode_hash FROM factory_deps WHERE miniblock_number > $1",
            block_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| H256::from_slice(&row.bytecode_hash))
        .collect()
    }

    /// Applies the specified storage logs for a miniblock. Returns the map of unique storage updates.
    // We likely don't need `storage` table at all, as we have `storage_logs` table
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

        // Copy from stdin can't be used here because of 'ON CONFLICT'.
        sqlx::query!(
            "INSERT INTO storage (hashed_key, address, key, value, tx_hash, created_at, updated_at) \
            SELECT u.hashed_key, u.address, u.key, u.value, u.tx_hash, now(), now() \
                FROM UNNEST ($1::bytea[], $2::bytea[], $3::bytea[], $4::bytea[], $5::bytea[]) \
                AS u(hashed_key, address, key, value, tx_hash) \
            ON CONFLICT (hashed_key) \
            DO UPDATE SET tx_hash = excluded.tx_hash, value = excluded.value, updated_at = now()",
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

    /// Gets the current storage value at the specified `key`.
    pub async fn get_by_key(&mut self, key: &StorageKey) -> Option<H256> {
        let hashed_key = key.hashed_key();

        sqlx::query!(
            "SELECT value FROM storage WHERE hashed_key = $1",
            hashed_key.as_bytes()
        )
        .instrument("get_by_key")
        .report_latency()
        .with_arg("key", &hashed_key)
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| H256::from_slice(&row.value))
    }

    /// Removes all factory deps with a miniblock number strictly greater than the specified `block_number`.
    pub async fn rollback_factory_deps(&mut self, block_number: MiniblockNumber) {
        sqlx::query!(
            "DELETE FROM factory_deps WHERE miniblock_number > $1",
            block_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConnectionPool;
    use zksync_types::{AccountTreeId, Address};

    #[tokio::test]
    async fn applying_storage_logs() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();

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
        assert_eq!(first_value, H256::repeat_byte(1));
        let second_value = conn.storage_dal().get_by_key(&second_key).await.unwrap();
        assert_eq!(second_value, H256::repeat_byte(2));
    }
}
