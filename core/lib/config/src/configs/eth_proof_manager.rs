use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EthProofManagerConfig {
    #[config(default_t = Duration::from_secs(10))]
    pub event_poll_interval: Duration,
    #[config(default_t = Duration::from_secs(10))]
    pub request_sending_interval: Duration,
    #[config(default_t = 1000)]
    pub event_expiration_blocks: u64,
    #[config(default_t = 1000000)]
    pub default_priority_fee_per_gas: u64,
    // USDC contract has 6 decimals, standard reward should be 4$
    #[config(default_t = 4000000)]
    pub max_reward: u64,
    #[config(default_t = Duration::from_secs(150))]
    pub acknowledgment_timeout: Duration,
    #[config(default_t = Duration::from_secs(4800))]
    pub proof_generation_timeout: Duration,
    #[config(default_t = Duration::from_secs(300))]
    pub picking_timeout: Duration,
    #[config(default_t = 100000000000)]
    pub max_acceptable_priority_fee_in_gwei: u64,
    #[config(default_t = 10)]
    pub max_tx_sending_attempts: u64,
    #[config(default_t = Duration::from_secs(1))]
    pub tx_sending_sleep: Duration,
    #[config(default_t = 10)]
    pub tx_receipt_checking_max_attempts: u64,
    #[config(default_t = Duration::from_secs(1))]
    pub tx_receipt_checking_sleep: Duration,
    #[config(default_t = 1000000)]
    pub max_tx_gas: u64,
    #[config(default)]
    pub path_to_fflonk_verification_key: String,
}
