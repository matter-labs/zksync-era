use crate::StorageProcessor;
use sqlx::types::chrono::Utc;
use zksync_types::{
    get_code_key, Address, MiniblockNumber, StorageLog, FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
    H256,
};

#[derive(Debug)]
pub struct StorageLogsDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl StorageLogsDal<'_, '_> {
    pub fn insert_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
    ) {
        async_std::task::block_on(async {
            let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY storage_logs (hashed_key, address, key, value, operation_number, tx_hash, miniblock_number, created_at, updated_at)
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            let mut operation_number = 0u32;
            for (tx_hash, logs) in logs {
                let tx_hash_str = format!("\\\\x{}", hex::encode(tx_hash.0));
                for log in logs {
                    let hashed_key_str = format!("\\\\x{}", hex::encode(log.key.hashed_key().0));
                    let address_str = format!("\\\\x{}", hex::encode(log.key.address().0));
                    let key_str = format!("\\\\x{}", hex::encode(log.key.key().0));
                    let value_str = format!("\\\\x{}", hex::encode(log.value.0));
                    let row = format!(
                        "{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                        hashed_key_str,
                        address_str,
                        key_str,
                        value_str,
                        operation_number,
                        tx_hash_str,
                        block_number,
                        now,
                        now
                    );
                    bytes.extend_from_slice(row.as_bytes());

                    operation_number += 1;
                }
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        })
    }

    pub fn append_storage_logs(
        &mut self,
        block_number: MiniblockNumber,
        logs: &[(H256, Vec<StorageLog>)],
    ) {
        async_std::task::block_on(async {
            let mut operation_number = sqlx::query!(
                r#"SELECT COUNT(*) as "count!" FROM storage_logs WHERE miniblock_number = $1"#,
                block_number.0 as i64
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count as u32;

            let mut copy = self
                .storage
                .conn()
                .copy_in_raw(
                    "COPY storage_logs (hashed_key, address, key, value, operation_number, tx_hash, miniblock_number, created_at, updated_at)
                FROM STDIN WITH (DELIMITER '|')",
                )
                .await
                .unwrap();

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            for (tx_hash, logs) in logs {
                let tx_hash_str = format!("\\\\x{}", hex::encode(tx_hash.0));
                for log in logs {
                    let hashed_key_str = format!("\\\\x{}", hex::encode(log.key.hashed_key().0));
                    let address_str = format!("\\\\x{}", hex::encode(log.key.address().0));
                    let key_str = format!("\\\\x{}", hex::encode(log.key.key().0));
                    let value_str = format!("\\\\x{}", hex::encode(log.value.0));
                    let row = format!(
                        "{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                        hashed_key_str,
                        address_str,
                        key_str,
                        value_str,
                        operation_number,
                        tx_hash_str,
                        block_number,
                        now,
                        now
                    );
                    bytes.extend_from_slice(row.as_bytes());

                    operation_number += 1;
                }
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        })
    }

    pub fn rollback_storage(&mut self, block_number: MiniblockNumber) {
        async_std::task::block_on(async {
            vlog::info!("fetching keys that were changed after given block number");
            let modified_keys: Vec<H256> = sqlx::query!(
                "SELECT DISTINCT ON (hashed_key) hashed_key FROM
                (SELECT * FROM storage_logs WHERE miniblock_number > $1) inn",
                block_number.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| H256::from_slice(&row.hashed_key))
            .collect();
            vlog::info!("loaded {:?} keys", modified_keys.len());

            for key in modified_keys {
                let previous_value: Option<H256> = sqlx::query!(
                    "select value from storage_logs where hashed_key = $1 and miniblock_number <= $2 order by miniblock_number desc, operation_number desc limit 1",
                    key.as_bytes(),
                    block_number.0 as i64
                )
                .fetch_optional(self.storage.conn())
                .await
                .unwrap()
                .map(|r| H256::from_slice(&r.value));
                match previous_value {
                    None => {
                        sqlx::query!("delete from storage where hashed_key = $1", key.as_bytes(),)
                            .execute(self.storage.conn())
                            .await
                            .unwrap()
                    }
                    Some(val) => sqlx::query!(
                        "update storage set value = $1 where hashed_key = $2",
                        val.as_bytes(),
                        key.as_bytes(),
                    )
                    .execute(self.storage.conn())
                    .await
                    .unwrap(),
                };
            }
        })
    }

    pub fn rollback_storage_logs(&mut self, block_number: MiniblockNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM storage_logs WHERE miniblock_number > $1",
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn is_contract_deployed_at_address(&mut self, address: Address) -> bool {
        let hashed_key = get_code_key(&address).hashed_key();
        async_std::task::block_on(async {
            let count = sqlx::query!(
                r#"
                    SELECT COUNT(*) as "count!"
                    FROM (
                        SELECT * FROM storage_logs
                        WHERE storage_logs.hashed_key = $1
                        ORDER BY storage_logs.miniblock_number DESC, storage_logs.operation_number DESC
                        LIMIT 1
                    ) sl
                    WHERE sl.value != $2
                "#,
                hashed_key.as_bytes(),
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count;
            count > 0
        })
    }
}
