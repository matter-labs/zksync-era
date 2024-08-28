use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use zksync_basic_types::H256;
use zksync_crypto_primitives::K256PrivateKey;

/// By default, the ratio persister will run every 30 seconds.
const DEFAULT_PRICE_POLLING_INTERVAL_MS: u64 = 30_000;

/// By default, refetch ratio from db every 0.5 second
const DEFAULT_PRICE_CACHE_UPDATE_INTERVAL_MS: u64 = 500;

/// Default max amount of gas that a L1 base token update can consume per transaction
const DEFAULT_MAX_TX_GAS: u64 = 80_000;

/// Default priority fee per gas used to instantiate the signing client
const DEFAULT_PRIORITY_FEE_PER_GAS: u64 = 1_000_000_000;

/// Default maximum number of attempts to get L1 transaction receipt
const DEFAULT_L1_RECEIPT_CHECKING_MAX_ATTEMPTS: u32 = 3;

/// Default maximum number of attempts to submit L1 transaction
const DEFAULT_L1_TX_SENDING_MAX_ATTEMPTS: u32 = 3;

/// Default number of milliseconds to sleep between receipt checking attempts
const DEFAULT_L1_RECEIPT_CHECKING_SLEEP_MS: u64 = 30_000;

/// Default maximum number of attempts to fetch price from a remote API
const DEFAULT_PRICE_FETCHING_MAX_ATTEMPTS: u32 = 3;

/// Default number of milliseconds to sleep between price fetching attempts
const DEFAULT_PRICE_FETCHING_SLEEP_MS: u64 = 5_000;

/// Default number of milliseconds to sleep between transaction sending attempts
const DEFAULT_L1_TX_SENDING_SLEEP_MS: u64 = 30_000;

/// Default maximum acceptable priority fee in gwei to prevent sending transaction with extremely high priority fee.
const DEFAULT_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI: u64 = 100_000_000_000;

/// Default value for halting on error
const DEFAULT_HALT_ON_ERROR: bool = false;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
    #[serde(default = "BaseTokenAdjusterConfig::default_price_polling_interval_ms")]
    pub price_polling_interval_ms: u64,

    /// We (in memory) cache the ratio fetched from db. This interval defines frequency of refetch from db.
    #[serde(default = "BaseTokenAdjusterConfig::default_price_cache_update_interval_ms")]
    pub price_cache_update_interval_ms: u64,

    /// Max amount of gas that L1 base token update can consume per transaction
    #[serde(default = "BaseTokenAdjusterConfig::default_max_tx_gas")]
    pub max_tx_gas: u64,

    /// Default priority fee per gas used to instantiate the signing client
    #[serde(default = "BaseTokenAdjusterConfig::default_priority_fee_per_gas")]
    pub default_priority_fee_per_gas: u64,

    /// Maximum acceptable priority fee in gwei to prevent sending transaction with extremely high priority fee.
    #[serde(default = "BaseTokenAdjusterConfig::default_max_acceptable_priority_fee_in_gwei")]
    pub max_acceptable_priority_fee_in_gwei: u64,

    /// Maximum number of attempts to get L1 transaction receipt before failing over
    #[serde(default = "BaseTokenAdjusterConfig::default_l1_receipt_checking_max_attempts")]
    pub l1_receipt_checking_max_attempts: u32,

    /// Number of seconds to sleep between the receipt checking attempts
    #[serde(default = "BaseTokenAdjusterConfig::default_l1_receipt_checking_sleep_ms")]
    pub l1_receipt_checking_sleep_ms: u64,

    /// Maximum number of attempts to submit L1 transaction before failing over
    #[serde(default = "BaseTokenAdjusterConfig::default_l1_tx_sending_max_attempts")]
    pub l1_tx_sending_max_attempts: u32,

    /// Number of seconds to sleep between the transaction sending attempts
    #[serde(default = "BaseTokenAdjusterConfig::default_l1_tx_sending_sleep_ms")]
    pub l1_tx_sending_sleep_ms: u64,

    /// Maximum number of attempts to fetch quote from a remote API before failing over
    #[serde(default = "BaseTokenAdjusterConfig::default_price_fetching_max_attempts")]
    pub price_fetching_max_attempts: u32,

    /// Number of seconds to sleep between price fetching attempts
    #[serde(default = "BaseTokenAdjusterConfig::default_price_fetching_sleep_ms")]
    pub price_fetching_sleep_ms: u64,

    /// Defines whether base_token_adjuster should halt the process if there was an error while
    /// fetching or persisting the quote. Generally that should be set to false to not to halt
    /// the server process if an external api is not available or if L1 is congested.
    #[serde(default = "BaseTokenAdjusterConfig::default_halt_on_error")]
    pub halt_on_error: bool,
}

impl Default for BaseTokenAdjusterConfig {
    fn default() -> Self {
        Self {
            price_polling_interval_ms: Self::default_price_polling_interval_ms(),
            price_cache_update_interval_ms: Self::default_price_cache_update_interval_ms(),
            max_tx_gas: Self::default_max_tx_gas(),
            default_priority_fee_per_gas: Self::default_priority_fee_per_gas(),
            max_acceptable_priority_fee_in_gwei: Self::default_max_acceptable_priority_fee_in_gwei(
            ),
            l1_receipt_checking_max_attempts: Self::default_l1_receipt_checking_max_attempts(),
            l1_receipt_checking_sleep_ms: Self::default_l1_receipt_checking_sleep_ms(),
            l1_tx_sending_max_attempts: Self::default_l1_tx_sending_max_attempts(),
            l1_tx_sending_sleep_ms: Self::default_l1_tx_sending_sleep_ms(),
            price_fetching_sleep_ms: Self::default_price_fetching_sleep_ms(),
            price_fetching_max_attempts: Self::default_price_fetching_max_attempts(),
            halt_on_error: Self::default_halt_on_error(),
        }
    }
}

impl BaseTokenAdjusterConfig {
    pub fn default_price_polling_interval_ms() -> u64 {
        DEFAULT_PRICE_POLLING_INTERVAL_MS
    }

    pub fn price_polling_interval(&self) -> Duration {
        Duration::from_millis(self.price_polling_interval_ms)
    }

    pub fn default_price_cache_update_interval_ms() -> u64 {
        DEFAULT_PRICE_CACHE_UPDATE_INTERVAL_MS
    }

    pub fn default_priority_fee_per_gas() -> u64 {
        DEFAULT_PRIORITY_FEE_PER_GAS
    }

    pub fn default_max_acceptable_priority_fee_in_gwei() -> u64 {
        DEFAULT_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI
    }

    pub fn default_halt_on_error() -> bool {
        DEFAULT_HALT_ON_ERROR
    }

    pub fn price_cache_update_interval(&self) -> Duration {
        Duration::from_millis(self.price_cache_update_interval_ms)
    }

    pub fn l1_receipt_checking_sleep_duration(&self) -> Duration {
        Duration::from_millis(self.l1_receipt_checking_sleep_ms)
    }

    pub fn l1_tx_sending_sleep_duration(&self) -> Duration {
        Duration::from_millis(self.l1_tx_sending_sleep_ms)
    }

    pub fn price_fetching_sleep_duration(&self) -> Duration {
        Duration::from_millis(self.price_fetching_sleep_ms)
    }

    pub fn default_l1_receipt_checking_max_attempts() -> u32 {
        DEFAULT_L1_RECEIPT_CHECKING_MAX_ATTEMPTS
    }

    pub fn default_l1_receipt_checking_sleep_ms() -> u64 {
        DEFAULT_L1_RECEIPT_CHECKING_SLEEP_MS
    }

    pub fn default_l1_tx_sending_max_attempts() -> u32 {
        DEFAULT_L1_TX_SENDING_MAX_ATTEMPTS
    }

    pub fn default_l1_tx_sending_sleep_ms() -> u64 {
        DEFAULT_L1_TX_SENDING_SLEEP_MS
    }

    pub fn default_price_fetching_sleep_ms() -> u64 {
        DEFAULT_PRICE_FETCHING_SLEEP_MS
    }

    pub fn default_price_fetching_max_attempts() -> u32 {
        DEFAULT_PRICE_FETCHING_MAX_ATTEMPTS
    }

    pub fn default_max_tx_gas() -> u64 {
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
