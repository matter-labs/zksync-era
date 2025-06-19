use std::collections::HashMap;

use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{
    get_code_key, get_nonce_key, h256_to_u256,
    utils::{decompose_full_nonce, storage_key_for_standard_token_balance},
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, Nonce, StorageKey,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H256, U256,
};

use crate::{models::storage_block::ResolvedL1BatchForL2Block, Core, CoreDal};

/// Raw bytecode information returned by [`StorageWeb3Dal::get_contract_code_unchecked()`].
#[derive(Debug)]
pub struct RawBytecode {
    pub bytecode_hash: H256,
    pub bytecode: Vec<u8>,
}

#[derive(Debug)]
pub struct StorageWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl StorageWeb3Dal<'_, '_> {
    pub async fn get_address_historical_nonce(
        &mut self,
        address: Address,
        block_number: L2BlockNumber,
    ) -> DalResult<U256> {
        let nonce_key = get_nonce_key(&address);
        let nonce_value = self
            .get_historical_value_unchecked(nonce_key.hashed_key(), block_number)
            .await?;
        let full_nonce = h256_to_u256(nonce_value);
        Ok(decompose_full_nonce(full_nonce).0)
    }

    /// Returns the current *stored* nonces (i.e., w/o accounting for pending transactions) for the specified accounts.
    pub async fn get_nonces_for_addresses(
        &mut self,
        addresses: &[Address],
    ) -> DalResult<HashMap<Address, Nonce>> {
        //note: this is only used on EN (tx_sender/proxy.rs), so we don't adopt it
        let nonce_keys: HashMap<_, _> = addresses
            .iter()
            .map(|address| (get_nonce_key(address).hashed_key(), *address))
            .collect();

        let res = self
            .get_values(&nonce_keys.keys().copied().collect::<Vec<_>>())
            .await?
            .into_iter()
            .filter_map(|(hashed_key, value)| {
                let address = nonce_keys.get(&hashed_key)?;
                let full_nonce = h256_to_u256(value);
                let (nonce, _) = decompose_full_nonce(full_nonce);
                Some((*address, Nonce(nonce.as_u32())))
            })
            .collect();
        Ok(res)
    }

    pub async fn standard_token_historical_balance(
        &mut self,
        token_id: AccountTreeId,
        account_id: AccountTreeId,
        block_number: L2BlockNumber,
    ) -> DalResult<U256> {
        let key = storage_key_for_standard_token_balance(token_id, account_id.address());
        let balance = self
            .get_historical_value_unchecked(key.hashed_key(), block_number)
            .await?;
        Ok(h256_to_u256(balance))
    }

    /// Gets the current value for the specified `key`. Uses state of the latest sealed L2 block.
    /// Returns error if there is no sealed L2 blocks.
    pub async fn get_value(&mut self, key: &StorageKey) -> DalResult<H256> {
        let Some(l2_block_number) = self
            .storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await?
        else {
            let err = Instrumented::new("get_value")
                .with_arg("key", &key)
                .constraint_error(anyhow::anyhow!("no sealed l2 blocks"));
            return Err(err);
        };
        self.get_historical_value_unchecked(key.hashed_key(), l2_block_number)
            .await
    }

    /// Gets the current values for the specified `hashed_keys`. The returned map has requested hashed keys as keys
    /// and current storage values as values. Uses state of the latest sealed L2 block.
    /// Returns error if there is no sealed L2 blocks.
    pub async fn get_values(&mut self, hashed_keys: &[H256]) -> DalResult<HashMap<H256, H256>> {
        let Some(l2_block_number) = self
            .storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await?
        else {
            let err = Instrumented::new("get_values")
                .with_arg("hashed_keys", &hashed_keys)
                .constraint_error(anyhow::anyhow!("no sealed l2 blocks"));
            return Err(err);
        };
        let storage_map = self
            .storage
            .storage_logs_dal()
            .get_storage_values(hashed_keys, l2_block_number)
            .await?;
        Ok(storage_map
            .into_iter()
            .map(|(key, value)| (key, value.unwrap_or_default()))
            .collect())
    }

    /// This method does not check if a block with this number exists in the database.
    /// It will return the current value if the block is in the future.
    pub async fn get_historical_value_unchecked(
        &mut self,
        hashed_key: H256,
        block_number: L2BlockNumber,
    ) -> DalResult<H256> {
        sqlx::query!(
            r#"
            SELECT
                value
            FROM
                storage_logs
            WHERE
                storage_logs.hashed_key = $1
                AND storage_logs.miniblock_number <= $2
            ORDER BY
                storage_logs.miniblock_number DESC,
                storage_logs.operation_number DESC
            LIMIT
                1
            "#,
            hashed_key.as_bytes(),
            i64::from(block_number.0)
        )
        .instrument("get_historical_value_unchecked")
        .report_latency()
        .with_arg("key", &hashed_key)
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage)
        .await
        .map(|option_row| {
            option_row
                .map(|row| H256::from_slice(&row.value))
                .unwrap_or_else(H256::zero)
        })
    }
    /// This method does not check if a block with this number exists in the database.
    /// It will return the current value if the block is in the future.
    pub async fn get_historical_option_value_unchecked(
        &mut self,
        hashed_key: H256,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<H256>> {
        sqlx::query!(
            r#"
            SELECT
                value
            FROM
                storage_logs
            WHERE
                storage_logs.hashed_key = $1
                AND storage_logs.miniblock_number <= $2
            ORDER BY
                storage_logs.miniblock_number DESC,
                storage_logs.operation_number DESC
            LIMIT
                1
            "#,
            hashed_key.as_bytes(),
            i64::from(block_number.0)
        )
        .instrument("get_historical_value_unchecked")
        .report_latency()
        .with_arg("key", &hashed_key)
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage)
        .await
        .map(|option_row| option_row.map(|row| H256::from_slice(&row.value)))
    }

    /// Provides information about the L1 batch that the specified L2 block is a part of.
    /// Assumes that the L2 block is present in the DB; this is not checked, and if this is false,
    /// the returned value will be meaningless.
    pub async fn resolve_l1_batch_number_of_l2_block(
        &mut self,
        l2_block_number: L2BlockNumber,
    ) -> DalResult<ResolvedL1BatchForL2Block> {
        let row = sqlx::query!(
            r#"
            SELECT
                (
                    SELECT
                        l1_batch_number
                    FROM
                        miniblocks
                    WHERE
                        number = $1
                ) AS "block_batch?",
                COALESCE(
                    (
                        SELECT
                            MAX(number) + 1
                        FROM
                            l1_batches
                        WHERE
                            is_sealed
                    ),
                    (
                        SELECT
                            MAX(l1_batch_number) + 1
                        FROM
                            snapshot_recovery
                    ),
                    0
                ) AS "pending_batch!"
            "#,
            i64::from(l2_block_number.0)
        )
        .instrument("resolve_l1_batch_number_of_l2_block")
        .with_arg("l2_block_number", &l2_block_number)
        .fetch_one(self.storage)
        .await?;

        Ok(ResolvedL1BatchForL2Block {
            block_l1_batch: row.block_batch.map(|n| L1BatchNumber(n as u32)),
            pending_l1_batch: L1BatchNumber(row.pending_batch as u32),
        })
    }

    pub async fn get_l1_batch_number_for_initial_write(
        &mut self,
        hashed_key: H256,
    ) -> DalResult<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                initial_writes
            WHERE
                hashed_key = $1
            "#,
            hashed_key.as_bytes(),
        )
        .instrument("get_l1_batch_number_for_initial_write")
        .report_latency()
        .with_arg("key", &hashed_key)
        .fetch_optional(self.storage)
        .await?;

        let l1_batch_number = row.map(|record| L1BatchNumber(record.l1_batch_number as u32));
        Ok(l1_batch_number)
    }

    /// This method doesn't check if block with number equals to `block_number`
    /// is present in the database. For such blocks `None` will be returned.
    pub async fn get_contract_code_unchecked(
        &mut self,
        address: Address,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<RawBytecode>> {
        let hashed_key = get_code_key(&address).hashed_key();
        let row = sqlx::query!(
            r#"
            SELECT
                bytecode_hash,
                bytecode
            FROM
                (
                    SELECT
                        value
                    FROM
                        storage_logs
                    WHERE
                        storage_logs.hashed_key = $1
                        AND storage_logs.miniblock_number <= $2
                    ORDER BY
                        storage_logs.miniblock_number DESC,
                        storage_logs.operation_number DESC
                    LIMIT
                        1
                ) deploy_log
            JOIN factory_deps ON value = factory_deps.bytecode_hash
            WHERE
                value != $3
            "#,
            hashed_key.as_bytes(),
            i64::from(block_number.0),
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes(),
        )
        .instrument("get_contract_code_unchecked")
        .with_arg("address", &address)
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| RawBytecode {
            bytecode_hash: H256::from_slice(&row.bytecode_hash),
            bytecode: row.bytecode,
        }))
    }

    /// Given bytecode hash, returns bytecode and L2 block number at which it was inserted.
    pub async fn get_factory_dep(
        &mut self,
        hash: H256,
    ) -> DalResult<Option<(Vec<u8>, L2BlockNumber)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                bytecode,
                miniblock_number
            FROM
                factory_deps
            WHERE
                bytecode_hash = $1
            "#,
            hash.as_bytes(),
        )
        .instrument("get_factory_dep")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| (row.bytecode, L2BlockNumber(row.miniblock_number as u32))))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{block::L1BatchHeader, ProtocolVersion, ProtocolVersionId};

    use super::*;
    use crate::{
        tests::{create_l2_block_header, create_snapshot_recovery},
        ConnectionPool, Core, CoreDal,
    };

    #[tokio::test]
    async fn resolving_l1_batch_number_of_l2_block() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();
        let l1_batch_header = L1BatchHeader::new(
            L1BatchNumber(0),
            0,
            Default::default(),
            ProtocolVersionId::latest(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();

        let first_l2_block = create_l2_block_header(1);
        conn.blocks_dal()
            .insert_l2_block(&first_l2_block)
            .await
            .unwrap();

        let resolved = conn
            .storage_web3_dal()
            .resolve_l1_batch_number_of_l2_block(L2BlockNumber(0))
            .await
            .unwrap();
        assert_eq!(resolved.block_l1_batch, Some(L1BatchNumber(0)));
        assert_eq!(resolved.pending_l1_batch, L1BatchNumber(1));
        assert_eq!(resolved.expected_l1_batch(), L1BatchNumber(0));

        let timestamp = conn
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&resolved)
            .await
            .unwrap();
        assert_eq!(timestamp, Some(0));

        for pending_l2_block_number in [1, 2] {
            let resolved = conn
                .storage_web3_dal()
                .resolve_l1_batch_number_of_l2_block(L2BlockNumber(pending_l2_block_number))
                .await
                .unwrap();
            assert_eq!(resolved.block_l1_batch, None);
            assert_eq!(resolved.pending_l1_batch, L1BatchNumber(1));
            assert_eq!(resolved.expected_l1_batch(), L1BatchNumber(1));

            let timestamp = conn
                .blocks_web3_dal()
                .get_expected_l1_batch_timestamp(&resolved)
                .await
                .unwrap();
            assert_eq!(timestamp, Some(first_l2_block.timestamp));
        }
    }

    #[tokio::test]
    async fn resolving_l1_batch_number_of_l2_block_with_snapshot_recovery() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        let snapshot_recovery = create_snapshot_recovery();
        conn.snapshot_recovery_dal()
            .insert_initial_recovery_status(&snapshot_recovery)
            .await
            .unwrap();

        let first_l2_block = create_l2_block_header(snapshot_recovery.l2_block_number.0 + 1);
        conn.blocks_dal()
            .insert_l2_block(&first_l2_block)
            .await
            .unwrap();

        let resolved = conn
            .storage_web3_dal()
            .resolve_l1_batch_number_of_l2_block(snapshot_recovery.l2_block_number + 1)
            .await
            .unwrap();
        assert_eq!(resolved.block_l1_batch, None);
        assert_eq!(
            resolved.pending_l1_batch,
            snapshot_recovery.l1_batch_number + 1
        );
        assert_eq!(
            resolved.expected_l1_batch(),
            snapshot_recovery.l1_batch_number + 1
        );

        let timestamp = conn
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&resolved)
            .await
            .unwrap();
        assert_eq!(timestamp, Some(first_l2_block.timestamp));

        let l1_batch_header = L1BatchHeader::new(
            snapshot_recovery.l1_batch_number + 1,
            100,
            Default::default(),
            ProtocolVersionId::latest(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_header.number)
            .await
            .unwrap();

        let resolved = conn
            .storage_web3_dal()
            .resolve_l1_batch_number_of_l2_block(snapshot_recovery.l2_block_number + 1)
            .await
            .unwrap();
        assert_eq!(resolved.block_l1_batch, Some(l1_batch_header.number));
        assert_eq!(resolved.pending_l1_batch, l1_batch_header.number + 1);
        assert_eq!(resolved.expected_l1_batch(), l1_batch_header.number);

        let timestamp = conn
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&resolved)
            .await
            .unwrap();
        assert_eq!(timestamp, Some(first_l2_block.timestamp));
    }
}
