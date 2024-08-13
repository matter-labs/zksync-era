use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use zksync_basic_types::H256;
use zksync_crypto_primitives::K256PrivateKey;

/// By default, the ratio persister will run every 30 seconds.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

/// By default, refetch ratio from db every 0.5 second
pub const DEFAULT_CACHE_UPDATE_INTERVAL: u64 = 500;

/// Default max amount of gas that a L1 base token update can consume per transaction
pub const DEFAULT_MAX_TX_GAS: u64 = 80_000;

/// Default max amount of gas that a L1 base token update can consume per transaction
pub const DEFAULT_PRIORITY_FEE_PER_GAS: u64 = 1_000_000_000;

/// Default maximum number of attempts to get L1 transaction receipt
const DEFAULT_L1_RECEIPT_CHECKING_MAX_ATTEMPTS: u32 = 3;

/// Default maximum number of attempts to submit L1 transaction
const DEFAULT_L1_TX_SENDING_MAX_ATTEMPTS: u32 = 3;

/// Default number of milliseconds to sleep between receipt checking attempts
const DEFAULT_L1_RECEIPT_CHECKING_SLEEP_MS: u64 = 30_000;

/// Default number of milliseconds to sleep between transaction sending attempts
const DEFAULT_L1_TX_SENDING_SLEEP_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
    #[serde(default = "BaseTokenAdjusterConfig::default_polling_interval")]
    pub price_polling_interval_ms: u64,

    /// We (in memory) cache the ratio fetched from db. This interval defines frequency of refetch from db.
    #[serde(default = "BaseTokenAdjusterConfig::default_cache_update_interval")]
    pub price_cache_update_interval_ms: u64,

    /// Max amount of gas that L1 base token update can consume per transaction
    #[serde(default = "BaseTokenAdjusterConfig::default_max_tx_gas")]
    pub max_tx_gas: u64,

    /// Default priority fee per gas used to instantiate the signing client
    #[serde(default = "BaseTokenAdjusterConfig::default_priority_fee_per_gas")]
    pub default_priority_fee_per_gas: u64,

    /// Maximum number of attempts to get L1 transaction receipt before failing over
    pub l1_receipt_checking_max_attempts: Option<u32>,

    /// Number of seconds to sleep between the receipt checking attempts
    pub l1_receipt_checking_sleep_ms: Option<u64>,

    /// Maximum number of attempts to submit L1 transaction before failing over
    pub l1_tx_sending_max_attempts: Option<u32>,

    /// Number of seconds to sleep between the transaction sending attempts
    pub l1_tx_sending_sleep_ms: Option<u64>,
}

impl Default for BaseTokenAdjusterConfig {
    fn default() -> Self {
        Self {
            price_polling_interval_ms: Self::default_polling_interval(),
            price_cache_update_interval_ms: Self::default_cache_update_interval(),
            max_tx_gas: Self::default_max_tx_gas(),
            default_priority_fee_per_gas: Self::default_priority_fee_per_gas(),
            l1_receipt_checking_max_attempts: Some(DEFAULT_L1_RECEIPT_CHECKING_MAX_ATTEMPTS),
            l1_receipt_checking_sleep_ms: Some(DEFAULT_L1_RECEIPT_CHECKING_SLEEP_MS),
            l1_tx_sending_max_attempts: Some(DEFAULT_L1_TX_SENDING_MAX_ATTEMPTS),
            l1_tx_sending_sleep_ms: Some(DEFAULT_L1_TX_SENDING_SLEEP_MS),
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

    fn default_priority_fee_per_gas() -> u64 {
        DEFAULT_PRIORITY_FEE_PER_GAS
    }

    pub fn price_cache_update_interval(&self) -> Duration {
        Duration::from_millis(self.price_cache_update_interval_ms)
    }

    pub fn l1_tx_sending_max_attempts(&self) -> u32 {
        self.l1_tx_sending_max_attempts
            .map_or(DEFAULT_L1_TX_SENDING_MAX_ATTEMPTS, |x| x)
    }

    pub fn l1_receipt_checking_max_attempts(&self) -> u32 {
        self.l1_receipt_checking_max_attempts
            .map_or(DEFAULT_L1_RECEIPT_CHECKING_MAX_ATTEMPTS, |x| x)
    }

    pub fn l1_receipt_checking_sleep_duration(&self) -> Duration {
        Duration::from_millis(
            self.l1_receipt_checking_sleep_ms
                .map_or(DEFAULT_L1_RECEIPT_CHECKING_SLEEP_MS, |x| x),
        )
    }

    pub fn l1_tx_sending_sleep_duration(&self) -> Duration {
        Duration::from_millis(
            self.l1_tx_sending_sleep_ms
                .map_or(DEFAULT_L1_TX_SENDING_SLEEP_MS, |x| x),
        )
    }

    fn default_max_tx_gas() -> u64 {
        DEFAULT_MAX_TX_GAS
    }

    pub fn private_key(&self) -> anyhow::Result<Option<K256PrivateKey>> {
        std::env::var("TOKEN_MULTIPLIER_SETTER_PRIVATE_KEY")
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
