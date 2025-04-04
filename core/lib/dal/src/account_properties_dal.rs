use std::{collections::HashMap, convert::TryInto};

use anyhow::Context as _;
use bigdecimal::BigDecimal;
use itertools::Itertools;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{
    boojum_os::AccountProperties, Address, L1BatchNumber, L2BlockNumber, H256, U256,
};

use crate::{
    models::{
        bigdecimal_to_u256, storage_account_properties::StorageAccountProperties,
        u256_to_big_decimal,
    },
    Core, CoreDal,
};

#[derive(Debug)]
pub struct AccountPropertiesDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl AccountPropertiesDal<'_, '_> {
    pub async fn insert_account_properties(
        &mut self,
        l2_block_number: L2BlockNumber,
        properties: &[(Address, AccountProperties)],
    ) -> DalResult<()> {
        let (
            addresses,
            preimage_hashes,
            versioning_data,
            nonces,
            observable_bytecode_hashes,
            bytecode_hashes,
            balances,
            bytecode_lens,
            artifacts_lens,
            observable_bytecode_lens,
        ): (
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
            Vec<_>,
        ) = properties
            .iter()
            .map(|(addr, p)| {
                (
                    addr.as_bytes(),
                    p.hash().0.to_vec(),
                    BigDecimal::from(p.versioning_data),
                    BigDecimal::from(p.nonce),
                    p.observable_bytecode_hash.as_bytes(),
                    p.bytecode_hash.as_bytes(),
                    u256_to_big_decimal(p.nominal_token_balance),
                    p.bytecode_len as i64,
                    p.artifacts_len as i64,
                    p.observable_bytecode_len as i64,
                )
            })
            .multiunzip();
        sqlx::query!(
            r#"
            INSERT INTO account_properties (
                address,
                miniblock_number,
                preimage_hash,
                versioning_data,
                nonce,
                observable_bytecode_hash,
                bytecode_hash,
                nominal_token_balance,
                bytecode_len,
                artifacts_len,
                observable_bytecode_len
            )
            SELECT
                u.address,
                $1,
                u.preimage_hash,
                u.versioning_data,
                u.nonce,
                u.observable_bytecode_hash,
                u.bytecode_hash,
                u.nominal_token_balance,
                u.bytecode_len,
                u.artifacts_len,
                u.observable_bytecode_len
            FROM
                UNNEST(
                    $2::bytea [],
                    $3::bytea [],
                    $4::numeric [],
                    $5::numeric [],
                    $6::bytea [],
                    $7::bytea [],
                    $8::numeric [],
                    $9::bigint [],
                    $10::bigint [],
                    $11::bigint []
                ) AS u (
                    address,
                    preimage_hash,
                    versioning_data,
                    nonce,
                    observable_bytecode_hash,
                    bytecode_hash,
                    nominal_token_balance,
                    bytecode_len,
                    artifacts_len,
                    observable_bytecode_len
                )
            "#,
            i64::from(l2_block_number.0),
            &addresses as &[&[u8]],
            &preimage_hashes,
            &versioning_data,
            &nonces,
            &observable_bytecode_hashes as &[&[u8]],
            &bytecode_hashes as &[&[u8]],
            &balances,
            &bytecode_lens,
            &artifacts_lens,
            &observable_bytecode_lens,
        )
        .instrument("insert_account_properties")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    async fn get_account_properties(
        &mut self,
        address: Address,
        l2_block_number: Option<L2BlockNumber>,
    ) -> DalResult<Option<StorageAccountProperties>> {
        let l2_block_number = l2_block_number.map_or(i64::MAX, |n| i64::from(n.0));
        let result = sqlx::query_as!(
            StorageAccountProperties,
            r#"
            SELECT
                versioning_data,
                nonce,
                observable_bytecode_hash,
                bytecode_hash,
                nominal_token_balance,
                bytecode_len,
                artifacts_len,
                observable_bytecode_len
            FROM
                account_properties
            WHERE address = $1 AND miniblock_number <= $2
            ORDER BY
                miniblock_number DESC
            LIMIT
                1
            "#,
            address.as_bytes(),
            l2_block_number,
        )
        .instrument("get_account_balance")
        .fetch_optional(self.storage)
        .await?;

        Ok(result)
    }

    pub async fn get_balance(
        &mut self,
        address: Address,
        l2_block_number: Option<L2BlockNumber>,
    ) -> DalResult<U256> {
        let balance = self
            .get_account_properties(address, l2_block_number)
            .await?
            .map(|p| bigdecimal_to_u256(p.nominal_token_balance))
            .unwrap_or_default();
        Ok(balance)
    }

    pub async fn get_nonce(
        &mut self,
        address: Address,
        l2_block_number: Option<L2BlockNumber>,
    ) -> DalResult<u64> {
        let balance = self
            .get_account_properties(address, l2_block_number)
            .await?
            .map(|p| bigdecimal_to_u256(p.nonce).as_u64())
            .unwrap_or_default();
        Ok(balance)
    }

    pub async fn roll_back_properties(&mut self, block_number: L2BlockNumber) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM account_properties
            WHERE
                miniblock_number > $1
            "#,
            i64::from(block_number.0)
        )
        .instrument("roll_back_properties")
        .with_arg("block_number", &block_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_l1_batch_account_properties(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<HashMap<H256, Vec<u8>>> {
        Ok(sqlx::query_as!(
            StorageAccountProperties,
            r#"
            SELECT
                versioning_data,
                nonce,
                observable_bytecode_hash,
                bytecode_hash,
                nominal_token_balance,
                bytecode_len,
                artifacts_len,
                observable_bytecode_len
            FROM
                account_properties
            INNER JOIN miniblocks ON miniblocks.number = account_properties.miniblock_number
            WHERE
                miniblocks.l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l1_batch_account_properties")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|properties: StorageAccountProperties| {
            let properties = AccountProperties::from(properties);
            (properties.hash(), properties.encode().to_vec())
        })
        .collect())
    }

    pub async fn account_properties_by_hash(
        &mut self,
        hash: H256,
    ) -> DalResult<Option<AccountProperties>> {
        Ok(sqlx::query_as!(
            StorageAccountProperties,
            r#"
            SELECT
                versioning_data,
                nonce,
                observable_bytecode_hash,
                bytecode_hash,
                nominal_token_balance,
                bytecode_len,
                artifacts_len,
                observable_bytecode_len
            FROM
                account_properties
            WHERE
                preimage_hash = $1
            "#,
            hash.as_bytes()
        )
        .instrument("account_properties_by_hash")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await?
        .map(Into::into))
    }
}
