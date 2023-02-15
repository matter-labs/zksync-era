use crate::{SqlxError, StorageProcessor};
use std::time::Instant;
use zksync_types::{
    api::BlockId,
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, storage_key_for_standard_token_balance},
    AccountTreeId, Address, StorageKey, FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H256, U256,
};
use zksync_utils::h256_to_u256;
use zksync_web3_decl::error::Web3Error;

#[derive(Debug)]
pub struct StorageWeb3Dal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl StorageWeb3Dal<'_, '_> {
    pub fn get_address_historical_nonce(
        &mut self,
        address: Address,
        block_id: BlockId,
    ) -> Result<Result<U256, Web3Error>, SqlxError> {
        let nonce_key = get_nonce_key(&address);
        let nonce = self.get_historical_value(&nonce_key, block_id)?.map(|n| {
            let full_nonce = h256_to_u256(n);
            decompose_full_nonce(full_nonce).0
        });
        Ok(nonce)
    }

    pub fn standard_token_historical_balance(
        &mut self,
        token_id: AccountTreeId,
        account_id: AccountTreeId,
        block_id: BlockId,
    ) -> Result<Result<U256, Web3Error>, SqlxError> {
        let key = storage_key_for_standard_token_balance(token_id, account_id.address());

        let balance = self.get_historical_value(&key, block_id)?;
        Ok(balance.map(h256_to_u256))
    }

    pub fn get_historical_value(
        &mut self,
        key: &StorageKey,
        block_id: BlockId,
    ) -> Result<Result<H256, Web3Error>, SqlxError> {
        let block_number = self.storage.blocks_web3_dal().resolve_block_id(block_id)?;
        match block_number {
            Ok(block_number) => {
                let value = self.get_historical_value_unchecked(key, block_number)?;
                Ok(Ok(value))
            }
            Err(err) => Ok(Err(err)),
        }
    }

    /// This method does not check if a block with this number exists in the database.
    /// It will return the current value if the block is in the future.
    pub fn get_historical_value_unchecked(
        &mut self,
        key: &StorageKey,
        block_number: zksync_types::MiniblockNumber,
    ) -> Result<H256, SqlxError> {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            // We need to proper distinguish if the value is zero or None
            // for the VM to correctly determine initial writes.
            // So, we accept that the value is None if it's zero and it wasn't initially written at the moment.
            let result = sqlx::query!(
                r#"
                SELECT value
                FROM storage_logs
                WHERE storage_logs.hashed_key = $1 AND storage_logs.miniblock_number <= $2
                ORDER BY storage_logs.miniblock_number DESC, storage_logs.operation_number DESC
                LIMIT 1
                "#,
                key.hashed_key().0.to_vec(),
                block_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await
            .map(|option_row| {
                option_row
                    .map(|row| H256::from_slice(&row.value))
                    .unwrap_or_else(H256::zero)
            });
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_historical_value_unchecked");

            result
        })
    }

    pub fn is_write_initial(
        &mut self,
        key: &StorageKey,
        block_number: zksync_types::MiniblockNumber,
        consider_new_l1_batch: bool,
    ) -> Result<bool, SqlxError> {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let row = sqlx::query!(
                r#"
                SELECT (SELECT l1_batch_number FROM initial_writes WHERE hashed_key = $1) as "initial_write_l1_batch_number?",
                    (SELECT miniblocks.l1_batch_number FROM miniblocks WHERE number = $2) as "current_l1_batch_number?"
                "#,
                key.hashed_key().0.to_vec(),
                block_number.0 as i64
            )
                .fetch_one(self.storage.conn())
                .await?;
            // Note: if `row.current_l1_batch_number` is `None` it means
            // that the l1 batch that the miniblock is included in isn't sealed yet.
            let is_initial = match (
                row.current_l1_batch_number,
                row.initial_write_l1_batch_number,
            ) {
                (_, None) => true,
                (Some(current_l1_batch_number), Some(initial_write_l1_batch_number)) => {
                    if consider_new_l1_batch {
                        current_l1_batch_number < initial_write_l1_batch_number
                    } else {
                        current_l1_batch_number <= initial_write_l1_batch_number
                    }
                }
                (None, Some(_initial_write_l1_batch_number)) => false,
            };
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "is_write_initial");

            Ok(is_initial)
        })
    }

    pub fn get_contract_code(
        &mut self,
        address: Address,
        block_id: BlockId,
    ) -> Result<Result<Option<Vec<u8>>, Web3Error>, SqlxError> {
        let block_number = self.storage.blocks_web3_dal().resolve_block_id(block_id)?;
        match block_number {
            Ok(block_number) => {
                let code = self.get_contract_code_unchecked(address, block_number)?;
                Ok(Ok(code))
            }
            Err(err) => Ok(Err(err)),
        }
    }

    /// This method doesn't check if block with number equals to `block_number`
    /// is present in the database. For such blocks `None` will be returned.
    pub fn get_contract_code_unchecked(
        &mut self,
        address: Address,
        block_number: zksync_types::MiniblockNumber,
    ) -> Result<Option<Vec<u8>>, SqlxError> {
        let hashed_key = get_code_key(&address).hashed_key();
        async_std::task::block_on(async {
            sqlx::query!(
                "
                    SELECT bytecode FROM (
                        SELECT * FROM storage_logs
                        WHERE
                            storage_logs.hashed_key = $1 AND
                            storage_logs.miniblock_number <= $2
                        ORDER BY
                            storage_logs.miniblock_number DESC, storage_logs.operation_number DESC
                        LIMIT 1
                    ) t
                    JOIN factory_deps ON value = factory_deps.bytecode_hash
                    WHERE value != $3
                ",
                hashed_key.as_bytes(),
                block_number.0 as i64,
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
            )
            .fetch_optional(self.storage.conn())
            .await
            .map(|option_row| option_row.map(|row| row.bytecode))
        })
    }

    /// This method doesn't check if block with number equals to `block_number`
    /// is present in the database. For such blocks `None` will be returned.
    pub fn get_factory_dep_unchecked(
        &mut self,
        hash: H256,
        block_number: zksync_types::MiniblockNumber,
    ) -> Result<Option<Vec<u8>>, SqlxError> {
        async_std::task::block_on(async {
            sqlx::query!(
                "SELECT bytecode FROM factory_deps WHERE bytecode_hash = $1 AND miniblock_number <= $2",
                &hash.0.to_vec(),
                block_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await
            .map(|option_row| option_row.map(|row| row.bytecode))
        })
    }
}
