use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use zksync_basic_types::H256;
use zksync_crypto_primitives::K256PrivateKey;

/// By default, the ratio persister will run every 30 seconds.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

/// By default, refetch ratio from db every 0.5 second
pub const DEFAULT_CACHE_UPDATE_INTERVAL: u64 = 500;

/// Default maximum number of attempts to get L1 transaction receipt
const DEFAULT_PERSISTER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS: u32 = 3;

/// Default number of seconds to sleep between the attempts
const DEFAULT_PERSISTER_L1_RECEIPT_CHECKING_SLEEP_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
    #[serde(default = "BaseTokenAdjusterConfig::default_polling_interval")]
    pub price_polling_interval_ms: u64,

    /// We (in memory) cache the ratio fetched from db. This interval defines frequency of refetch from db.
    #[serde(default = "BaseTokenAdjusterConfig::default_cache_update_interval")]
    pub price_cache_update_interval_ms: u64,

    /// Maximum number of attempts to get L1 transaction receipt before failing over
    pub persister_l1_receipt_checking_max_attempts: Option<u32>,

    /// Number of seconds to sleep between the attempts
    pub persister_l1_receipt_checking_sleep_ms: Option<u64>,
}

impl Default for BaseTokenAdjusterConfig {
    fn default() -> Self {
        Self {
            price_polling_interval_ms: Self::default_polling_interval(),
            price_cache_update_interval_ms: Self::default_cache_update_interval(),
            persister_l1_receipt_checking_max_attempts: Some(
                DEFAULT_PERSISTER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS,
            ),
            persister_l1_receipt_checking_sleep_ms: Some(
                DEFAULT_PERSISTER_L1_RECEIPT_CHECKING_SLEEP_MS,
            ),
        }
    }
}

impl BaseTokenAdjusterConfig {
    fn default_polling_interval() -> u64 {
        DEFAULT_INTERVAL_MS
    }

    pub fn price_polling_interval(&self) -> Duration {
        Duration::from_millis(self.price_polling_interval_ms)
    }

    fn default_cache_update_interval() -> u64 {
        DEFAULT_CACHE_UPDATE_INTERVAL
    }

    pub fn price_cache_update_interval(&self) -> Duration {
        Duration::from_millis(self.price_cache_update_interval_ms)
    }

    pub fn persister_l1_receipt_checking_max_attempts(&self) -> u32 {
        self.persister_l1_receipt_checking_max_attempts
            .map_or(DEFAULT_PERSISTER_L1_RECEIPT_CHECKING_MAX_ATTEMPTS, |x| x)
    }

    pub fn persister_l1_receipt_checking_sleep_duration(&self) -> Duration {
        Duration::from_millis(
            self.persister_l1_receipt_checking_sleep_ms
                .map_or(DEFAULT_PERSISTER_L1_RECEIPT_CHECKING_SLEEP_MS, |x| x),
        )
    }

    #[deprecated]
    pub fn private_key(&self) -> anyhow::Result<Option<K256PrivateKey>> {
        std::env::var("BASE_TOKEN_ADJUSTER_PRIVATE_KEY")
            .ok()
            .map(|pk| {
                let private_key_bytes: H256 =
                    pk.parse().context("failed parsing private key bytes")?;
                K256PrivateKey::from_bytes(private_key_bytes)
                    .context("private key bytes are invalid")
            })
            .transpose()
    }
}
