/// External uses
use serde::Deserialize;
/// Built-in uses
use std::time::Duration;
// Local uses
use zksync_basic_types::network::Network;
use zksync_basic_types::{Address, H256};
use zksync_contracts::BaseSystemContractsHashes;

use crate::envy_load;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChainConfig {
    /// L1 parameters configuration.
    pub eth: Eth,
    /// State keeper / block generating configuration.
    pub state_keeper: StateKeeperConfig,
    /// Operations manager / Metadata calculator.
    pub operations_manager: OperationsManager,
    /// mempool configuration
    pub mempool: MempoolConfig,
    /// circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
}

impl ChainConfig {
    pub fn from_env() -> Self {
        Self {
            eth: envy_load!("eth", "CHAIN_ETH_"),
            state_keeper: envy_load!("state_keeper", "CHAIN_STATE_KEEPER_"),
            operations_manager: envy_load!("operations_manager", "CHAIN_OPERATIONS_MANAGER_"),
            mempool: envy_load!("mempool", "CHAIN_MEMPOOL_"),
            circuit_breaker: envy_load!("circuit_breaker", "CHAIN_CIRCUIT_BREAKER_"),
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Eth {
    /// Name of the used Ethereum network, e.g. `localhost` or `rinkeby`.
    pub network: Network,
    /// Name of current zkSync network
    /// Used for Sentry environment
    pub zksync_network: String,
    /// ID of current zkSync network treated as ETH network ID.
    /// Used to distinguish zkSync from other Web3-capable networks.
    pub zksync_network_id: u16,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct StateKeeperConfig {
    /// The max number of slots for txs in a block before it should be sealed by the slots sealer.
    pub transaction_slots: usize,

    /// Number of ms after which an L1 batch is going to be unconditionally sealed.
    pub block_commit_deadline_ms: u64,
    /// Number of ms after which a miniblock should be sealed by the timeout sealer.
    pub miniblock_commit_deadline_ms: u64,

    /// The max number of gas to spend on an L1 tx before its batch should be sealed by the gas sealer.
    pub max_single_tx_gas: u32,

    pub max_allowed_l2_tx_gas_limit: u32,

    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    pub reject_tx_at_geometry_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    pub reject_tx_at_eth_params_percentage: f64,
    /// Configuration option for tx to be rejected in case
    /// it takes more percentage of the block capacity than this value.
    pub reject_tx_at_gas_percentage: f64,
    /// Denotes the percentage of geometry params used in l2 block, that triggers l2 block seal.
    pub close_block_at_geometry_percentage: f64,
    /// Denotes the percentage of l1 params used in l2 block, that triggers l2 block seal.
    pub close_block_at_eth_params_percentage: f64,
    /// Denotes the percentage of l1 gas used in l2 block, that triggers l2 block seal.
    pub close_block_at_gas_percentage: f64,

    pub fee_account_addr: Address,

    /// The price the operator spends on 1 gas of computation in wei.
    pub fair_l2_gas_price: u64,

    pub bootloader_hash: H256,
    pub default_aa_hash: H256,

    /// Max number of computational gas that validation step is allowed to take.
    pub validation_computational_gas_limit: u32,
    pub save_call_traces: bool,
    /// Max number of l1 gas price that is allowed to be used in state keeper.
    pub max_l1_gas_price: Option<u64>,
}

impl StateKeeperConfig {
    pub fn max_l1_gas_price(&self) -> u64 {
        self.max_l1_gas_price.unwrap_or(u64::MAX)
    }

    pub fn base_system_contracts_hashes(&self) -> BaseSystemContractsHashes {
        BaseSystemContractsHashes {
            bootloader: self.bootloader_hash,
            default_aa: self.default_aa_hash,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct OperationsManager {
    /// Sleep time in ms when there is no new input data
    pub delay_interval: u64,
}

impl OperationsManager {
    pub fn delay_interval(&self) -> Duration {
        Duration::from_millis(self.delay_interval)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CircuitBreakerConfig {
    pub sync_interval_ms: u64,
    pub http_req_max_retry_number: usize,
    pub http_req_retry_interval_sec: u8,
}

impl CircuitBreakerConfig {
    pub fn sync_interval(&self) -> Duration {
        Duration::from_millis(self.sync_interval_ms)
    }

    pub fn http_req_retry_interval(&self) -> Duration {
        Duration::from_secs(self.http_req_retry_interval_sec as u64)
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct MempoolConfig {
    pub sync_interval_ms: u64,
    pub sync_batch_size: usize,
    pub capacity: u64,
    pub stuck_tx_timeout: u64,
    pub remove_stuck_txs: bool,
}

impl MempoolConfig {
    pub fn sync_interval(&self) -> Duration {
        Duration::from_millis(self.sync_interval_ms)
    }

    pub fn stuck_tx_timeout(&self) -> Duration {
        Duration::from_secs(self.stuck_tx_timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::{addr, set_env};

    fn expected_config() -> ChainConfig {
        ChainConfig {
            eth: Eth {
                network: "localhost".parse().unwrap(),
                zksync_network: "localhost".to_string(),
                zksync_network_id: 270,
            },
            state_keeper: StateKeeperConfig {
                transaction_slots: 50,
                block_commit_deadline_ms: 2500,
                miniblock_commit_deadline_ms: 1000,
                max_single_tx_gas: 1_000_000,
                max_allowed_l2_tx_gas_limit: 2_000_000_000,
                close_block_at_eth_params_percentage: 0.2,
                close_block_at_gas_percentage: 0.8,
                close_block_at_geometry_percentage: 0.5,
                reject_tx_at_eth_params_percentage: 0.8,
                reject_tx_at_geometry_percentage: 0.3,
                fee_account_addr: addr("de03a0B5963f75f1C8485B355fF6D30f3093BDE7"),
                reject_tx_at_gas_percentage: 0.5,
                fair_l2_gas_price: 250000000,
                bootloader_hash: H256::from(&[254; 32]),
                default_aa_hash: H256::from(&[254; 32]),
                validation_computational_gas_limit: 10_000_000,
                save_call_traces: false,
                max_l1_gas_price: Some(100000000),
            },
            operations_manager: OperationsManager {
                delay_interval: 100,
            },
            mempool: MempoolConfig {
                sync_interval_ms: 10,
                sync_batch_size: 1000,
                capacity: 1_000_000,
                stuck_tx_timeout: 10,
                remove_stuck_txs: true,
            },
            circuit_breaker: CircuitBreakerConfig {
                sync_interval_ms: 1000,
                http_req_max_retry_number: 5,
                http_req_retry_interval_sec: 2,
            },
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
CHAIN_ETH_NETWORK="localhost"
CHAIN_ETH_ZKSYNC_NETWORK="localhost"
CHAIN_ETH_ZKSYNC_NETWORK_ID=270
CHAIN_STATE_KEEPER_TRANSACTION_SLOTS="50"
CHAIN_STATE_KEEPER_FEE_ACCOUNT_ADDR="0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7"
CHAIN_STATE_KEEPER_MAX_SINGLE_TX_GAS="1000000"
CHAIN_STATE_KEEPER_MAX_ALLOWED_L2_TX_GAS_LIMIT="2000000000"
CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_GEOMETRY_PERCENTAGE="0.5"
CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_GAS_PERCENTAGE="0.8"
CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_ETH_PARAMS_PERCENTAGE="0.2"
CHAIN_STATE_KEEPER_REJECT_TX_AT_GEOMETRY_PERCENTAGE="0.3"
CHAIN_STATE_KEEPER_REJECT_TX_AT_ETH_PARAMS_PERCENTAGE="0.8"
CHAIN_STATE_KEEPER_REJECT_TX_AT_GAS_PERCENTAGE="0.5"
CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS="2500"
CHAIN_STATE_KEEPER_MINIBLOCK_COMMIT_DEADLINE_MS="1000"
CHAIN_STATE_KEEPER_FAIR_L2_GAS_PRICE="250000000"
CHAIN_STATE_KEEPER_BOOTLOADER_HASH="0xfefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefe"
CHAIN_STATE_KEEPER_DEFAULT_AA_HASH="0xfefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefe"
CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT="10000000"
CHAIN_STATE_KEEPER_SAVE_CALL_TRACES="false"
CHAIN_STATE_KEEPER_MAX_L1_GAS_PRICE="100000000"
CHAIN_OPERATIONS_MANAGER_DELAY_INTERVAL="100"
CHAIN_MEMPOOL_SYNC_INTERVAL_MS="10"
CHAIN_MEMPOOL_SYNC_BATCH_SIZE="1000"
CHAIN_MEMPOOL_STUCK_TX_TIMEOUT="10"
CHAIN_MEMPOOL_REMOVE_STUCK_TXS="true"
CHAIN_MEMPOOL_CAPACITY="1000000"
CHAIN_CIRCUIT_BREAKER_SYNC_INTERVAL_MS="1000"
CHAIN_CIRCUIT_BREAKER_HTTP_REQ_MAX_RETRY_NUMBER="5"
CHAIN_CIRCUIT_BREAKER_HTTP_REQ_RETRY_INTERVAL_SEC="2"
        "#;
        set_env(config);

        let actual = ChainConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
