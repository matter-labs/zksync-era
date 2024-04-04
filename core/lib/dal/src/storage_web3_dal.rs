use std::collections::HashMap;

use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{
    get_code_key, get_nonce_key,
    utils::{decompose_full_nonce, storage_key_for_standard_token_balance},
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, Nonce, StorageKey,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H256, U256,
};
use zksync_utils::h256_to_u256;

use crate::{models::storage_block::ResolvedL1BatchForMiniblock, Core, CoreDal, SqlxError};

#[derive(Debug)]
pub struct StorageWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl StorageWeb3Dal<'_, '_> {
    pub async fn get_address_historical_nonce(
        &mut self,
        address: Address,
        block_number: MiniblockNumber,
    ) -> DalResult<U256> {
        let nonce_key = get_nonce_key(&address);
        let nonce_value = self
            .get_historical_value_unchecked(&nonce_key, block_number)
            .await?;
        let full_nonce = h256_to_u256(nonce_value);
        Ok(decompose_full_nonce(full_nonce).0)
    }

    /// Returns the current *stored* nonces (i.e., w/o accounting for pending transactions) for the specified accounts.
    pub async fn get_nonces_for_addresses(
        &mut self,
        addresses: &[Address],
    ) -> DalResult<HashMap<Address, Nonce>> {
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
        block_number: MiniblockNumber,
    ) -> DalResult<U256> {
        let key = storage_key_for_standard_token_balance(token_id, account_id.address());
        let balance = self
            .get_historical_value_unchecked(&key, block_number)
            .await?;
        Ok(h256_to_u256(balance))
    }

    /// Gets the current value for the specified `key`.
    pub async fn get_value(&mut self, key: &StorageKey) -> DalResult<H256> {
        self.get_historical_value_unchecked(key, MiniblockNumber(u32::MAX))
            .await
    }

    /// Gets the current values for the specified `hashed_keys`. The returned map has requested hashed keys as keys
    /// and current storage values as values.
    pub async fn get_values(&mut self, hashed_keys: &[H256]) -> DalResult<HashMap<H256, H256>> {
        let storage_map = self
            .storage
            .storage_logs_dal()
            .get_storage_values(hashed_keys, MiniblockNumber(u32::MAX))
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
        key: &StorageKey,
        block_number: MiniblockNumber,
    ) -> DalResult<H256> {
        let hashed_key = key.hashed_key();

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

    /// Provides information about the L1 batch that the specified miniblock is a part of.
    /// Assumes that the miniblock is present in the DB; this is not checked, and if this is false,
    /// the returned value will be meaningless.
    pub async fn resolve_l1_batch_number_of_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Result<ResolvedL1BatchForMiniblock, SqlxError> {
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
            i64::from(miniblock_number.0)
        )
        .fetch_one(self.storage.conn())
        .await?;

        Ok(ResolvedL1BatchForMiniblock {
            miniblock_l1_batch: row.block_batch.map(|n| L1BatchNumber(n as u32)),
            pending_l1_batch: L1BatchNumber(row.pending_batch as u32),
        })
    }

    pub async fn get_l1_batch_number_for_initial_write(
        &mut self,
        key: &StorageKey,
    ) -> DalResult<Option<L1BatchNumber>> {
        let hashed_key = key.hashed_key();
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
        block_number: MiniblockNumber,
    ) -> DalResult<Option<Vec<u8>>> {
        let hashed_key = get_code_key(&address).hashed_key();
        let row = sqlx::query!(
            r#"
            SELECT
                bytecode
            FROM
                (
                    SELECT
                        *
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
                ) t
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
        Ok(row.map(|row| row.bytecode))
    }

    /// Given bytecode hash, returns `bytecode` and `miniblock_number` at which it was inserted.
    pub async fn get_factory_dep(
        &mut self,
        hash: H256,
    ) -> sqlx::Result<Option<(Vec<u8>, MiniblockNumber)>> {
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
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(row.map(|row| (row.bytecode, MiniblockNumber(row.miniblock_number as u32))))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{block::L1BatchHeader, ProtocolVersion, ProtocolVersionId};

    use super::*;
    use crate::{
        tests::{create_miniblock_header, create_snapshot_recovery},
        ConnectionPool, Core, CoreDal,
    };

    #[tokio::test]
    async fn resolving_l1_batch_number_of_miniblock() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
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
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();

        let first_miniblock = create_miniblock_header(1);
        conn.blocks_dal()
            .insert_miniblock(&first_miniblock)
            .await
            .unwrap();

        let resolved = conn
            .storage_web3_dal()
            .resolve_l1_batch_number_of_miniblock(MiniblockNumber(0))
            .await
            .unwrap();
        assert_eq!(resolved.miniblock_l1_batch, Some(L1BatchNumber(0)));
        assert_eq!(resolved.pending_l1_batch, L1BatchNumber(1));
        assert_eq!(resolved.expected_l1_batch(), L1BatchNumber(0));

        let timestamp = conn
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&resolved)
            .await
            .unwrap();
        assert_eq!(timestamp, Some(0));

        for pending_miniblock_number in [1, 2] {
            let resolved = conn
                .storage_web3_dal()
                .resolve_l1_batch_number_of_miniblock(MiniblockNumber(pending_miniblock_number))
                .await
                .unwrap();
            assert_eq!(resolved.miniblock_l1_batch, None);
            assert_eq!(resolved.pending_l1_batch, L1BatchNumber(1));
            assert_eq!(resolved.expected_l1_batch(), L1BatchNumber(1));

            let timestamp = conn
                .blocks_web3_dal()
                .get_expected_l1_batch_timestamp(&resolved)
                .await
                .unwrap();
            assert_eq!(timestamp, Some(first_miniblock.timestamp));
        }
    }

    #[tokio::test]
    async fn resolving_l1_batch_number_of_miniblock_with_snapshot_recovery() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        let snapshot_recovery = create_snapshot_recovery();
        conn.snapshot_recovery_dal()
            .insert_initial_recovery_status(&snapshot_recovery)
            .await
            .unwrap();

        let first_miniblock = create_miniblock_header(snapshot_recovery.miniblock_number.0 + 1);
        conn.blocks_dal()
            .insert_miniblock(&first_miniblock)
            .await
            .unwrap();

        let resolved = conn
            .storage_web3_dal()
            .resolve_l1_batch_number_of_miniblock(snapshot_recovery.miniblock_number + 1)
            .await
            .unwrap();
        assert_eq!(resolved.miniblock_l1_batch, None);
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
        assert_eq!(timestamp, Some(first_miniblock.timestamp));

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
            .mark_miniblocks_as_executed_in_l1_batch(l1_batch_header.number)
            .await
            .unwrap();

        let resolved = conn
            .storage_web3_dal()
            .resolve_l1_batch_number_of_miniblock(snapshot_recovery.miniblock_number + 1)
            .await
            .unwrap();
        assert_eq!(resolved.miniblock_l1_batch, Some(l1_batch_header.number));
        assert_eq!(resolved.pending_l1_batch, l1_batch_header.number + 1);
        assert_eq!(resolved.expected_l1_batch(), l1_batch_header.number);

        let timestamp = conn
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&resolved)
            .await
            .unwrap();
        assert_eq!(timestamp, Some(first_miniblock.timestamp));
    }
}
