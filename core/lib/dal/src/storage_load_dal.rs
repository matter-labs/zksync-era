use crate::StorageProcessor;
use std::time::Instant;
use zksync_state::secondary_storage::SecondaryStateStorage;
use zksync_storage::RocksDB;
use zksync_types::{
    AccountTreeId, Address, L1BatchNumber, StorageKey, StorageLog, ACCOUNT_CODE_STORAGE_ADDRESS,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H256,
};
use zksync_utils::h256_to_account_address;

#[derive(Debug)]
pub struct StorageLoadDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl StorageLoadDal<'_, '_> {
    pub fn load_secondary_storage(&mut self, db: RocksDB) -> SecondaryStateStorage {
        async_std::task::block_on(async {
            let stage_started_at: Instant = Instant::now();
            let latest_l1_batch_number = self.storage.blocks_dal().get_sealed_block_number();
            vlog::debug!(
                "loading storage for l1 batch number {}",
                latest_l1_batch_number.0
            );

            let mut result = SecondaryStateStorage::new(db);
            let mut current_l1_batch_number = result.get_l1_batch_number().0;

            assert!(
                current_l1_batch_number <= latest_l1_batch_number.0 + 1,
                "L1 batch number in state keeper cache is greater than last sealed L1 batch number in Postgres"
            );
            while current_l1_batch_number <= latest_l1_batch_number.0 {
                let (from_miniblock_number, to_miniblock_number) = self
                    .storage
                    .blocks_dal()
                    .get_miniblock_range_of_l1_batch(L1BatchNumber(current_l1_batch_number))
                    .expect("L1 batch should contain at least one miniblock");

                vlog::debug!(
                    "loading state changes for l1 batch {}",
                    current_l1_batch_number
                );
                let storage_logs: Vec<_> = sqlx::query!(
                    "
                        SELECT address, key, value FROM storage_logs
                        WHERE miniblock_number >= $1 AND miniblock_number <= $2
                        ORDER BY miniblock_number, operation_number ASC
                    ",
                    from_miniblock_number.0 as i64,
                    to_miniblock_number.0 as i64,
                )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .map(|row| {
                    StorageLog::new_write_log(
                        StorageKey::new(
                            AccountTreeId::new(Address::from_slice(&row.address)),
                            H256::from_slice(&row.key),
                        ),
                        H256::from_slice(&row.value),
                    )
                })
                .collect();
                result.process_transaction_logs(&storage_logs);

                vlog::debug!(
                    "loading deployed contracts for l1 batch {}",
                    current_l1_batch_number
                );
                sqlx::query!(
                    "
                        SELECT storage_logs.key, factory_deps.bytecode
                        FROM storage_logs
                        JOIN factory_deps ON storage_logs.value = factory_deps.bytecode_hash
                        WHERE
                            storage_logs.address = $1 AND
                            storage_logs.miniblock_number >= $3 AND
                            storage_logs.miniblock_number <= $4 AND
                            NOT EXISTS (
                                SELECT 1 FROM storage_logs as s
                                WHERE
                                    s.hashed_key = storage_logs.hashed_key AND
                                    (s.miniblock_number, s.operation_number) >= (storage_logs.miniblock_number, storage_logs.operation_number) AND
                                    s.value = $2
                            )
                    ",
                    ACCOUNT_CODE_STORAGE_ADDRESS.as_bytes(),
                    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
                    from_miniblock_number.0 as i64,
                    to_miniblock_number.0 as i64
                )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .for_each(|row| {
                    result.store_contract(
                        h256_to_account_address(&H256::from_slice(&row.key)),
                        row.bytecode,
                    )
                });

                vlog::debug!(
                    "loading factory deps for l1 batch {}",
                    current_l1_batch_number
                );
                sqlx::query!(
                    "SELECT bytecode_hash, bytecode FROM factory_deps
                    WHERE miniblock_number >= $1 AND miniblock_number <= $2",
                    from_miniblock_number.0 as i64,
                    to_miniblock_number.0 as i64
                )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .for_each(|row| {
                    result.store_factory_dep(H256::from_slice(&row.bytecode_hash), row.bytecode)
                });

                current_l1_batch_number += 1;
                result.save(L1BatchNumber(current_l1_batch_number));
            }

            metrics::histogram!(
                "server.state_keeper.update_secondary_storage",
                stage_started_at.elapsed()
            );
            result
        })
    }

    pub fn load_number_of_contracts(&mut self) -> u64 {
        async_std::task::block_on(async {
            sqlx::query!(
                "SELECT count(*)
                FROM storage
                WHERE
                    address = $1 AND
                    value != $2
                ",
                ACCOUNT_CODE_STORAGE_ADDRESS.as_bytes(),
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count
            .unwrap() as u64
        })
    }
}
