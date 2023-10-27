use crate::StorageProcessor;
use num::{rational::Ratio, BigUint};
use sqlx::types::chrono::Utc;
use zksync_types::{
    tokens::{TokenInfo, TokenMetadata, TokenPrice},
    Address, MiniblockNumber, ACCOUNT_CODE_STORAGE_ADDRESS,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
};
use zksync_utils::ratio_to_big_decimal;

// Precision of the USD price per token
pub(crate) const STORED_USD_PRICE_PRECISION: usize = 6;

#[derive(Debug)]
pub struct TokensDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TokensDal<'_, '_> {
    pub async fn add_tokens(&mut self, tokens: Vec<TokenInfo>) {
        {
            let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY tokens (l1_address, l2_address, name, symbol, decimals, well_known, created_at, updated_at)
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await
            .unwrap();

            let mut bytes: Vec<u8> = Vec::new();
            let now = Utc::now().naive_utc().to_string();
            for TokenInfo {
                l1_address,
                l2_address,
                metadata:
                    TokenMetadata {
                        name,
                        symbol,
                        decimals,
                    },
            } in tokens
            {
                let l1_address_str = format!("\\\\x{}", hex::encode(l1_address.0));
                let l2_address_str = format!("\\\\x{}", hex::encode(l2_address.0));
                let row = format!(
                    "{}|{}|{}|{}|{}|FALSE|{}|{}\n",
                    l1_address_str, l2_address_str, name, symbol, decimals, now, now
                );
                bytes.extend_from_slice(row.as_bytes());
            }
            copy.send(bytes).await.unwrap();
            copy.finish().await.unwrap();
        }
    }

    pub async fn update_well_known_l1_token(
        &mut self,
        l1_address: &Address,
        metadata: TokenMetadata,
    ) {
        {
            sqlx::query!(
                "UPDATE tokens SET token_list_name = $2, token_list_symbol = $3,
                token_list_decimals = $4, well_known = true, updated_at = now()
                WHERE l1_address = $1
                ",
                l1_address.as_bytes(),
                metadata.name,
                metadata.symbol,
                metadata.decimals as i32,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn get_well_known_token_addresses(&mut self) -> Vec<(Address, Address)> {
        {
            let records =
                sqlx::query!("SELECT l1_address, l2_address FROM tokens WHERE well_known = true")
                    .fetch_all(self.storage.conn())
                    .await
                    .unwrap();
            let addresses: Vec<(Address, Address)> = records
                .into_iter()
                .map(|record| {
                    (
                        Address::from_slice(&record.l1_address),
                        Address::from_slice(&record.l2_address),
                    )
                })
                .collect();
            addresses
        }
    }

    pub async fn get_all_l2_token_addresses(&mut self) -> Vec<Address> {
        {
            let records = sqlx::query!("SELECT l2_address FROM tokens")
                .fetch_all(self.storage.conn())
                .await
                .unwrap();
            let addresses: Vec<Address> = records
                .into_iter()
                .map(|record| Address::from_slice(&record.l2_address))
                .collect();
            addresses
        }
    }

    pub async fn get_unknown_l1_token_addresses(&mut self) -> Vec<Address> {
        {
            let records = sqlx::query!("SELECT l1_address FROM tokens WHERE well_known = false")
                .fetch_all(self.storage.conn())
                .await
                .unwrap();
            let addresses: Vec<Address> = records
                .into_iter()
                .map(|record| Address::from_slice(&record.l1_address))
                .collect();
            addresses
        }
    }

    pub async fn get_l1_tokens_by_volume(&mut self, min_volume: &Ratio<BigUint>) -> Vec<Address> {
        {
            let min_volume = ratio_to_big_decimal(min_volume, STORED_USD_PRICE_PRECISION);
            let records = sqlx::query!(
                "SELECT l1_address FROM tokens WHERE market_volume > $1",
                min_volume
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            let addresses: Vec<Address> = records
                .into_iter()
                .map(|record| Address::from_slice(&record.l1_address))
                .collect();
            addresses
        }
    }

    pub async fn set_l1_token_price(&mut self, l1_address: &Address, price: TokenPrice) {
        {
            sqlx::query!(
            "UPDATE tokens SET usd_price = $2, usd_price_updated_at = $3, updated_at = now() WHERE l1_address = $1",
            l1_address.as_bytes(),
            ratio_to_big_decimal(&price.usd_price, STORED_USD_PRICE_PRECISION),
            price.last_updated.naive_utc(),
        )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn rollback_tokens(&mut self, block_number: MiniblockNumber) {
        {
            sqlx::query!(
                "
                    DELETE FROM tokens 
                    WHERE l2_address IN
                    (
                        SELECT substring(key, 12, 20) FROM storage_logs 
                        WHERE storage_logs.address = $1 AND miniblock_number > $2 AND NOT EXISTS (
                            SELECT 1 FROM storage_logs as s
                            WHERE
                                s.hashed_key = storage_logs.hashed_key AND
                                (s.miniblock_number, s.operation_number) >= (storage_logs.miniblock_number, storage_logs.operation_number) AND
                                s.value = $3
                        )
                    )
                ",
                ACCOUNT_CODE_STORAGE_ADDRESS.as_bytes(),
                block_number.0 as i64,
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes()
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }
}
