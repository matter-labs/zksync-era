use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use zksync_types::base_token_price::BaseTokenPrice;

/// Represents a row in the `storage_base_token_price` table.
#[derive(Debug, Clone)]
pub struct StorageBaseTokenPrice {
    pub id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub ratio_timestamp: NaiveDateTime,
    pub base_token_price: BigDecimal,
    pub eth_price: BigDecimal,
    pub used_in_l1: bool,
}

impl From<StorageBaseTokenPrice> for BaseTokenPrice {
    fn from(row: StorageBaseTokenPrice) -> BaseTokenPrice {
        BaseTokenPrice {
            id: row.id,
            ratio_timestamp: row.ratio_timestamp.and_utc(),
            base_token_price: row.base_token_price,
            eth_price: row.eth_price,
            used_in_l1: row.used_in_l1,
        }
    }
}
