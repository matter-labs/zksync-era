use sqlx::types::{
    chrono::{DateTime, NaiveDateTime, Utc},
    BigDecimal,
};

use zksync_types::tokens::{TokenMarketVolume, TokenMetadata, TokenPrice};
use zksync_utils::big_decimal_to_ratio;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: i32,
}

impl From<StorageTokenMetadata> for TokenMetadata {
    fn from(metadata: StorageTokenMetadata) -> TokenMetadata {
        TokenMetadata {
            name: metadata.name,
            symbol: metadata.symbol,
            decimals: metadata.decimals as u8,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTokenPrice {
    pub usd_price: Option<BigDecimal>,
    pub usd_price_updated_at: Option<NaiveDateTime>,
}

impl From<StorageTokenPrice> for Option<TokenPrice> {
    fn from(price: StorageTokenPrice) -> Option<TokenPrice> {
        match (&price.usd_price, price.usd_price_updated_at) {
            (Some(usd_price), Some(updated_at)) => Some(TokenPrice {
                usd_price: big_decimal_to_ratio(usd_price).unwrap(),
                last_updated: DateTime::<Utc>::from_utc(updated_at, Utc),
            }),
            (None, None) => None,
            _ => {
                vlog::warn!(
                    "Found storage token with {:?} `usd_price` and {:?} `usd_price_updated_at`",
                    price.usd_price,
                    price.usd_price_updated_at
                );
                None
            }
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageTokenMarketVolume {
    pub market_volume: Option<BigDecimal>,
    pub market_volume_updated_at: Option<NaiveDateTime>,
}

impl From<StorageTokenMarketVolume> for Option<TokenMarketVolume> {
    fn from(market_volume: StorageTokenMarketVolume) -> Option<TokenMarketVolume> {
        market_volume
            .market_volume
            .as_ref()
            .map(|volume| TokenMarketVolume {
                market_volume: big_decimal_to_ratio(volume).unwrap(),
                last_updated: DateTime::<Utc>::from_utc(
                    market_volume
                        .market_volume_updated_at
                        .expect("If `market_volume` is Some then `updated_at` must be Some"),
                    Utc,
                ),
            })
    }
}
