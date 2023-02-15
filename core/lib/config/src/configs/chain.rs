/// External uses
use serde::Deserialize;
/// Built-in uses
use std::time::Duration;
// Local uses
use zksync_basic_types::network::Network;
use zksync_basic_types::Address;

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
    /// Detones the amount of slots for transactions in the block.
    pub transaction_slots: usize,

    pub block_commit_deadline_ms: u64,
    pub miniblock_commit_deadline_ms: u64,

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

    pub reexecute_each_tx: bool,
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
}

impl CircuitBreakerConfig {
    pub fn sync_interval(&self) -> Duration {
        Duration::from_millis(self.sync_interval_ms)
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
                reexecute_each_tx: true,
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
CHAIN_STATE_KEEPER_REEXECUTE_EACH_TX="true"
CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS="2500"
CHAIN_STATE_KEEPER_MINIBLOCK_COMMIT_DEADLINE_MS="1000"
CHAIN_OPERATIONS_MANAGER_DELAY_INTERVAL="100"
CHAIN_MEMPOOL_SYNC_INTERVAL_MS="10"
CHAIN_MEMPOOL_SYNC_BATCH_SIZE="1000"
CHAIN_MEMPOOL_STUCK_TX_TIMEOUT="10"
CHAIN_MEMPOOL_REMOVE_STUCK_TXS="true"
CHAIN_MEMPOOL_CAPACITY="1000000"
CHAIN_CIRCUIT_BREAKER_SYNC_INTERVAL_MS="1000"
        "#;
        set_env(config);

        let actual = ChainConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
