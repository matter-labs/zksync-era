use std::time::Duration;

use anyhow::Context;
use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_basic_types::H256;
use zksync_crypto_primitives::K256PrivateKey;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Millis)]
    pub price_polling_interval_ms: Duration,

    /// We (in memory) cache the ratio fetched from db. This interval defines frequency of refetch from db.
    #[config(default_t = Duration::from_millis(500), with = TimeUnit::Millis)]
    pub price_cache_update_interval_ms: Duration,

    /// Max amount of gas that L1 base token update can consume per transaction
    #[config(default_t = 80_000)]
    pub max_tx_gas: u64,

    /// Default priority fee per gas used to instantiate the signing client
    #[config(default_t = 1_000_000_000)]
    pub default_priority_fee_per_gas: u64,

    /// Maximum acceptable priority fee in gwei to prevent sending transaction with extremely high priority fee.
    #[config(default_t = 100_000_000_000)]
    pub max_acceptable_priority_fee_in_gwei: u64,

    /// Maximum number of attempts to get L1 transaction receipt before failing over
    #[config(default_t = 3)]
    pub l1_receipt_checking_max_attempts: u32,

    /// Number of seconds to sleep between the receipt checking attempts
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Millis)]
    pub l1_receipt_checking_sleep_ms: Duration,

    /// Maximum number of attempts to submit L1 transaction before failing over
    #[config(default_t = 3)]
    pub l1_tx_sending_max_attempts: u32,

    /// Number of seconds to sleep between the transaction sending attempts
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Millis)]
    pub l1_tx_sending_sleep_ms: Duration,

    /// How many percent a quote needs to change in order for update to be propagated to L1.
    /// Exists to save on gas.
    #[config(default_t = 10)]
    pub l1_update_deviation_percentage: u32,

    /// Maximum number of attempts to fetch quote from a remote API before failing over
    #[config(default_t = 3)]
    pub price_fetching_max_attempts: u32,

    /// Number of seconds to sleep between price fetching attempts
    #[config(default_t = Duration::from_secs(5), with = TimeUnit::Millis)]
    pub price_fetching_sleep_ms: Duration,

    /// Defines whether base_token_adjuster should halt the process if there was an error while
    /// fetching or persisting the quote. Generally that should be set to false to not to halt
    /// the server process if an external api is not available or if L1 is congested.
    #[config(default_t = false)]
    pub halt_on_error: bool,
}

impl BaseTokenAdjusterConfig {
    // FIXME: OwO what's this?
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
