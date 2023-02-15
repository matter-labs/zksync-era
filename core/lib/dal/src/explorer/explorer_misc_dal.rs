use crate::explorer::storage_contract_info::StorageContractInfo;
use crate::SqlxError;
use crate::StorageProcessor;
use zksync_types::{
    explorer_api::{ContractBasicInfo, ContractStats, ExplorerTokenInfo},
    get_code_key, Address, FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
};

#[derive(Debug)]
pub struct ExplorerMiscDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ExplorerMiscDal<'_, '_> {
    pub fn get_token_details(
        &mut self,
        address: Address,
    ) -> Result<Option<ExplorerTokenInfo>, SqlxError> {
        async_std::task::block_on(async {
            let row = sqlx::query!(
                r#"
                SELECT l1_address, l2_address, symbol, name, decimals, usd_price
                FROM tokens
                WHERE l2_address = $1
                "#,
                address.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await?;
            let result = row.map(|row| ExplorerTokenInfo {
                l1_address: Address::from_slice(&row.l1_address),
                l2_address: Address::from_slice(&row.l2_address),
                address: Address::from_slice(&row.l2_address),
                symbol: row.symbol,
                name: row.name,
                decimals: row.decimals as u8,
                usd_price: row.usd_price,
            });
            Ok(result)
        })
    }

    pub fn get_well_known_token_l2_addresses(&mut self) -> Result<Vec<Address>, SqlxError> {
        async_std::task::block_on(async {
            let addresses = sqlx::query!("SELECT l2_address FROM tokens WHERE well_known = true")
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|record| Address::from_slice(&record.l2_address))
                .collect();
            Ok(addresses)
        })
    }

    pub fn get_contract_info(
        &mut self,
        address: Address,
    ) -> Result<Option<ContractBasicInfo>, SqlxError> {
        async_std::task::block_on(async {
            let hashed_key = get_code_key(&address).hashed_key();
            let info = sqlx::query_as!(
                StorageContractInfo,
                r#"
                    WITH sl AS (
                        SELECT * FROM storage_logs
                        WHERE storage_logs.hashed_key = $1
                        ORDER BY storage_logs.miniblock_number DESC, storage_logs.operation_number DESC
                        LIMIT 1
                    )
                    SELECT
                        sl.key as "key_address",
                        fd.bytecode,
                        txs.initiator_address as "creator_address?",
                        txs.hash as "creator_tx_hash?",
                        sl.miniblock_number as "created_in_block_number",
                        c.verification_info
                    FROM sl
                    JOIN factory_deps fd ON fd.bytecode_hash = sl.value
                    LEFT JOIN transactions txs ON txs.hash = sl.tx_hash
                    LEFT JOIN contracts_verification_info c ON c.address = $2
                    WHERE sl.value != $3
                "#,
                hashed_key.as_bytes(),
                address.as_bytes(),
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await?;
            Ok(info.map(|info| info.into()))
        })
    }

    pub fn get_contract_stats(&mut self, address: Address) -> Result<ContractStats, SqlxError> {
        async_std::task::block_on(async {
            let row = sqlx::query!(
                r#"
                SELECT COUNT(*) as "total_transactions!"
                FROM transactions
                WHERE contract_address = $1
                "#,
                address.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await?;
            let result = row
                .map(|row| ContractStats {
                    total_transactions: row.total_transactions as usize,
                })
                .unwrap_or_default();
            Ok(result)
        })
    }
}
