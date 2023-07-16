use std::collections::HashMap;

use zksync_types::{
    api,
    explorer_api::{AccountType, BalanceItem, ExplorerTokenInfo},
    get_code_key,
    tokens::ETHEREUM_ADDRESS,
    utils::storage_key_for_standard_token_balance,
    AccountTreeId, Address, Nonce, FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, L2_ETH_TOKEN_ADDRESS,
    U256,
};

use crate::{SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct ExplorerAccountsDal<'a, 'c> {
    pub(super) storage: &'a mut StorageProcessor<'c>,
}

impl ExplorerAccountsDal<'_, '_> {
    pub async fn get_balances_for_address(
        &mut self,
        address: Address,
    ) -> Result<HashMap<Address, BalanceItem>, SqlxError> {
        {
            let token_l2_addresses = self
                .storage
                .explorer()
                .misc_dal()
                .get_well_known_token_l2_addresses()
                .await?;
            let hashed_keys: Vec<Vec<u8>> = token_l2_addresses
                .into_iter()
                .map(|mut l2_token_address| {
                    if l2_token_address == ETHEREUM_ADDRESS {
                        l2_token_address = L2_ETH_TOKEN_ADDRESS;
                    }
                    storage_key_for_standard_token_balance(
                        AccountTreeId::new(l2_token_address),
                        &address,
                    )
                    .hashed_key()
                    .0
                    .to_vec()
                })
                .collect();
            let rows = sqlx::query!(
                r#"
                    SELECT storage.value as "value!",
                        tokens.l1_address as "l1_address!", tokens.l2_address as "l2_address!",
                        tokens.symbol as "symbol!", tokens.name as "name!", tokens.decimals as "decimals!", tokens.usd_price as "usd_price?"
                        FROM storage
                    INNER JOIN tokens ON
                        storage.address = tokens.l2_address OR (storage.address = $2 AND tokens.l2_address = $3)
                    WHERE storage.hashed_key = ANY($1)
                "#,
                &hashed_keys,
                L2_ETH_TOKEN_ADDRESS.as_bytes(),
                ETHEREUM_ADDRESS.as_bytes(),
            )
            .fetch_all(self.storage.conn())
            .await?;
            let result = rows
                .into_iter()
                .filter_map(|row| {
                    let balance = U256::from_big_endian(&row.value);
                    if balance.is_zero() {
                        None
                    } else {
                        let l2_address = Address::from_slice(&row.l2_address);
                        let token_info = ExplorerTokenInfo {
                            l1_address: Address::from_slice(&row.l1_address),
                            l2_address,
                            address: l2_address,
                            symbol: row.symbol,
                            name: row.name,
                            decimals: row.decimals as u8,
                            usd_price: row.usd_price,
                        };
                        let balance_item = BalanceItem {
                            token_info,
                            balance,
                        };
                        Some((l2_address, balance_item))
                    }
                })
                .collect();
            Ok(result)
        }
    }

    /// Returns sealed and verified nonces for address.
    pub async fn get_account_nonces(
        &mut self,
        address: Address,
    ) -> Result<(Nonce, Nonce), SqlxError> {
        let latest_block_number = self
            .storage
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await?
            .unwrap();
        let sealed_nonce = self
            .storage
            .storage_web3_dal()
            .get_address_historical_nonce(address, latest_block_number)
            .await?
            .as_u32();

        let finalized_block_number = self
            .storage
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Finalized))
            .await?
            .unwrap(); // Safe: we always have at least the genesis miniblock finalized
        let verified_nonce = self
            .storage
            .storage_web3_dal()
            .get_address_historical_nonce(address, finalized_block_number)
            .await?
            .as_u32();
        Ok((Nonce(sealed_nonce), Nonce(verified_nonce)))
    }

    pub async fn get_account_type(&mut self, address: Address) -> Result<AccountType, SqlxError> {
        let hashed_key = get_code_key(&address).hashed_key();
        {
            let contract_exists = sqlx::query!(
                r#"
                    SELECT true as "exists"
                    FROM (
                        SELECT * FROM storage_logs
                        WHERE hashed_key = $1
                        ORDER BY miniblock_number DESC, operation_number DESC
                        LIMIT 1
                    ) sl
                    WHERE sl.value != $2
                "#,
                hashed_key.as_bytes(),
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await?;
            let result = match contract_exists {
                Some(_) => AccountType::Contract,
                None => AccountType::EOA,
            };
            Ok(result)
        }
    }
}
