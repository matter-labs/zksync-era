use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

use crate::ObjectStoreConfig;
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EthProofManagerConfig {
    /// Chain id of settlement layer(where contracts are deployed)
    #[config(default_t = 271)]
    pub sl_chain_id: u64,
    /// HTTP RPC URL of settlement layer(where contracts are deployed)
    #[config(default_t = "http://127.0.0.1:8545".to_string())]
    pub http_rpc_url: String,
    /// Object store config
    #[config(nest, default)]
    pub object_store: ObjectStoreConfig,
    /// Interval for polling for new events
    #[config(default_t = Duration::from_secs(10))]
    pub event_poll_interval: Duration,
    /// Interval for sending requests to proof manager
    #[config(default_t = Duration::from_secs(10))]
    pub request_sending_interval: Duration,
    /// Number of blocks after which event is considered expired
    #[config(default_t = 1000)]
    pub event_expiration_blocks: u64,
    /// Default priority fee per gas
    #[config(default_t = 1000000)]
    pub default_priority_fee_per_gas: u64,
    /// Maximum reward for proof request
    // USDC contract has 6 decimals, standard reward should be 4$
    #[config(default_t = 4000000)]
    pub max_reward: u64,
    /// Timeout of waiting for acknowledgment event from proof manager
    #[config(default_t = Duration::from_secs(150))]
    pub acknowledgment_timeout: Duration,
    /// Timeout of waiting for proof to be generated
    #[config(default_t = Duration::from_secs(4800))]
    pub proof_generation_timeout: Duration,
    /// Timeout of waiting for proof to be picked up by prover
    #[config(default_t = Duration::from_secs(300))]
    pub picking_timeout: Duration,
    /// Maximum acceptable priority fee in gweiMaximum acceptable priority fee in gwei
    #[config(default_t = 100000000000)]
    pub max_acceptable_priority_fee_in_gwei: u64,
    /// Maximum number of attempts to send transaction
    #[config(default_t = 10)]
    pub max_tx_sending_attempts: u64,
    /// Sleep interval between sending transactions
    #[config(default_t = Duration::from_secs(1))]
    pub tx_sending_sleep: Duration,
    /// Maximum number of attempts to check transaction receipt
    #[config(default_t = 10)]
    pub tx_receipt_checking_max_attempts: u64,
    /// Sleep interval between checking transaction receipt
    #[config(default_t = Duration::from_secs(1))]
    pub tx_receipt_checking_sleep: Duration,
    /// Maximum gas for transaction
    #[config(default_t = 1000000)]
    pub max_tx_gas: u64,
    /// Path to fflonk verification key
    #[config(default)]
    pub path_to_fflonk_verification_key: String,
}
