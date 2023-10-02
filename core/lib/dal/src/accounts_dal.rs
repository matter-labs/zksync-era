use std::collections::HashMap;

use zksync_types::{
    tokens::ETHEREUM_ADDRESS, utils::storage_key_for_standard_token_balance, AccountTreeId,
    Address, L2_ETH_TOKEN_ADDRESS, U256,
};

use crate::{SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct AccountsDal<'a, 'c> {
    pub(super) storage: &'a mut StorageProcessor<'c>,
}

impl AccountsDal<'_, '_> {
    pub async fn get_balances_for_address(
        &mut self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, SqlxError> {
        let token_l2_addresses: Vec<Address> = self
            .storage
            .tokens_dal()
            .get_well_known_token_addresses()
            .await
            .into_iter()
            .map(|(_, l2_address)| l2_address)
            .collect();

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
                WHERE storage.hashed_key = ANY($1) AND storage.value != $4
            "#,
            &hashed_keys,
            L2_ETH_TOKEN_ADDRESS.as_bytes(),
            ETHEREUM_ADDRESS.as_bytes(),
            vec![0u8; 32]
        )
        .fetch_all(self.storage.conn())
        .await?;

        let result: HashMap<Address, U256> = rows
            .into_iter()
            .map(|row| {
                let balance = U256::from_big_endian(&row.value);
                (Address::from_slice(&row.l2_address), balance)
            })
            .collect();

        Ok(result)
    }
}
