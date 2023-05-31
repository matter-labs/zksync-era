use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use zksync_types::network::Network;
use zksync_types::{Address, L2ChainId, H160};

use crate::fs_utils::read_tokens;
use zksync_utils::test_utils::LoadnextContractExecutionParams;

/// Configuration for the loadtest.
///
/// This structure is meant to provide the least possible amount of parameters:
/// By the ideology of the test, it is OK for it to be opinionated. Thus we don't provide
/// kinds of operations we want to perform, do not configure fail or pass criteria.
///
/// It is expected that the user will provide the basic settings, and the loadtest will
/// take care of everything else.
#[derive(Debug, Clone, Deserialize)]
pub struct LoadtestConfig {
    /// Address of the Ethereum web3 API.
    #[serde(default = "default_l1_rpc_address")]
    pub l1_rpc_address: String,

    /// Ethereum private key of the wallet that has funds to perform a test.
    #[serde(default = "default_master_wallet_pk")]
    pub master_wallet_pk: String,

    /// Amount of accounts to be used in test.
    /// This option configures the "width" of the test:
    /// how many concurrent operation flows will be executed.
    #[serde(default = "default_accounts_amount")]
    pub accounts_amount: usize,

    /// Duration of the test.
    #[serde(default = "default_duration_sec")]
    pub duration_sec: u64,

    /// Address of the ERC-20 token to be used in test.
    ///
    /// Token must satisfy two criteria:
    /// - Be supported by zkSync.
    /// - Have `mint` operation.
    ///
    /// Note that we use ERC-20 token since we can't easily mint a lot of ETH on
    /// Rinkeby or Ropsten without caring about collecting it back.
    #[serde(default = "default_main_token")]
    pub main_token: Address,

    /// Path to test contracts bytecode and ABI required for sending
    /// deploy and execute L2 transactions. Each folder in the path is expected
    /// to have the following structure:
    ///```ignore
    /// .
    /// ├── bytecode
    /// └── abi.json
    ///```
    /// Contract folder names names are not restricted.
    ///
    /// An example:
    ///```ignore
    /// .
    /// ├── erc-20
    /// │   ├── bytecode
    /// │   └── abi.json
    /// └── simple-contract
    ///     ├── bytecode
    ///     └── abi.json
    ///```
    #[serde(default = "default_test_contracts_path")]
    pub test_contracts_path: PathBuf,
    /// Limits the number of simultaneous API requests being performed at any moment of time.
    ///
    /// Setting it to:
    /// - 0 turns off API requests.
    /// - `accounts_amount` relieves the limit.
    #[serde(default = "default_sync_api_requests_limit")]
    pub sync_api_requests_limit: usize,

    /// Limits the number of simultaneous active PubSub subscriptions at any moment of time.
    ///
    /// Setting it to:
    /// - 0 turns off PubSub subscriptions.
    #[serde(default = "default_sync_pubsub_subscriptions_limit")]
    pub sync_pubsub_subscriptions_limit: usize,

    /// Time in seconds for a subscription to be active. Subscription will be closed after that time.
    #[serde(default = "default_single_subscription_time_secs")]
    pub single_subscription_time_secs: u64,

    /// Optional seed to be used in the test: normally you don't need to set the seed,
    /// but you can re-use seed from previous run to reproduce the sequence of operations locally.
    /// Seed must be represented as a hexadecimal string.
    ///
    /// Using the same seed doesn't guarantee reproducibility of API requests: unlike operations, these
    /// are generated in flight by multiple accounts in parallel.
    #[serde(default = "default_seed")]
    pub seed: Option<String>,

    /// Chain id of L2 node.
    #[serde(default = "default_l2_chain_id")]
    pub l2_chain_id: u16,

    /// RPC address of L2 node.
    #[serde(default = "default_l2_rpc_address")]
    pub l2_rpc_address: String,

    /// WS RPC address of L2 node.
    #[serde(default = "default_l2_ws_rpc_address")]
    pub l2_ws_rpc_address: String,

    /// Explorer api address of L2 node.
    #[serde(default = "default_l2_explorer_api_address")]
    pub l2_explorer_api_address: String,

    /// The maximum number of transactions per account that can be sent without waiting for confirmation
    #[serde(default = "default_max_inflight_txs")]
    pub max_inflight_txs: usize,

    /// All of test accounts get split into groups that share the
    /// deployed contract address. This helps to emulate the behavior of
    /// sending `Execute` to the same contract and reading its events by
    /// single a group. This value should be less than or equal to `ACCOUNTS_AMOUNT`.
    #[serde(default = "default_accounts_group_size")]
    pub accounts_group_size: usize,

    /// The expected number of the processed transactions during loadtest
    /// that should be compared to the actual result.
    #[serde(default = "default_expected_tx_count")]
    pub expected_tx_count: Option<usize>,

    /// Label to use for results pushed to Prometheus.
    #[serde(default = "default_prometheus_label")]
    pub prometheus_label: String,
}

fn default_max_inflight_txs() -> usize {
    let result = 5;
    vlog::info!("Using default MAX_INFLIGHT_TXS: {}", result);
    result
}

fn default_l1_rpc_address() -> String {
    // https://rinkeby.infura.io/v3/8934c959275444d480834ba1587c095f for rinkeby
    let result = "http://127.0.0.1:8545".to_string();
    vlog::info!("Using default L1_RPC_ADDRESS: {}", result);
    result
}

fn default_l2_explorer_api_address() -> String {
    let result = "http://127.0.0.1:3070".to_string();
    vlog::info!("Using default L2_EXPLORER_API_ADDRESS: {}", result);
    result
}

fn default_master_wallet_pk() -> String {
    // Use this key only for localhost because it is compromised!
    // Using this key for rinkeby will result in losing rinkeby ETH.
    // Corresponding wallet is 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049
    let result = "7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110".to_string();
    vlog::info!("Using default MASTER_WALLET_PK: {}", result);
    result
}

fn default_accounts_amount() -> usize {
    let result = 80;
    vlog::info!("Using default ACCOUNTS_AMOUNT: {}", result);
    result
}

fn default_duration_sec() -> u64 {
    let result = 300;
    vlog::info!("Using default DURATION_SEC: {}", result);
    result
}

fn default_accounts_group_size() -> usize {
    let result = 40;
    vlog::info!("Using default ACCOUNTS_GROUP_SIZE: {}", result);
    result
}

fn default_main_token() -> H160 {
    // Read token addresses from `etc/tokens/localhost.json`. Use the first one
    // as a main token since all of them are suitable.

    // 0xeb8f08a975Ab53E34D8a0330E0D34de942C95926 for rinkeby
    let tokens = read_tokens(Network::Localhost).expect("Failed to parse tokens file");
    let main_token = tokens.first().expect("Loaded tokens list is empty");
    vlog::info!("Main token: {:?}", main_token);
    main_token.address
}

fn default_test_contracts_path() -> PathBuf {
    let test_contracts_path = {
        let home = std::env::var("ZKSYNC_HOME").unwrap();
        let path = PathBuf::from(&home);
        path.join("etc/contracts-test-data")
    };

    vlog::info!("Test contracts path: {}", test_contracts_path.display());

    test_contracts_path
}

fn default_sync_api_requests_limit() -> usize {
    let result = 20;
    vlog::info!("Using default SYNC_API_REQUESTS_LIMIT: {}", result);
    result
}

fn default_sync_pubsub_subscriptions_limit() -> usize {
    let result = 150;
    vlog::info!("Using default SYNC_PUBSUB_SUBSCRIPTIONS_LIMIT: {}", result);
    result
}

fn default_single_subscription_time_secs() -> u64 {
    let result = 30;
    vlog::info!("Using default SINGLE_SUBSCRIPTION_TIME_SECS: {}", result);
    result
}

fn default_seed() -> Option<String> {
    let result = None;
    vlog::info!("Using default SEED: {:?}", result);
    result
}

fn default_l2_chain_id() -> u16 {
    // 270 for rinkeby
    let result = *L2ChainId::default();
    vlog::info!("Using default L2_CHAIN_ID: {}", result);
    result
}

pub fn get_default_l2_rpc_address() -> String {
    "http://127.0.0.1:3050".to_string()
}

fn default_l2_rpc_address() -> String {
    // https://z2-dev-api.zksync.dev:443 for stage2
    let result = get_default_l2_rpc_address();
    vlog::info!("Using default L2_RPC_ADDRESS: {}", result);
    result
}

fn default_l2_ws_rpc_address() -> String {
    // ws://z2-dev-api.zksync.dev:80/ws for stage2
    let result = "ws://127.0.0.1:3051".to_string();
    vlog::info!("Using default L2_WS_RPC_ADDRESS: {}", result);
    result
}

fn default_expected_tx_count() -> Option<usize> {
    let result = None;
    vlog::info!("Using default EXPECTED_TX_COUNT: {:?}", result);
    result
}

fn default_prometheus_label() -> String {
    let result = "unset".to_string();
    vlog::info!("Using default PROMETHEUS_LABEL: {:?}", result);
    result
}

impl LoadtestConfig {
    pub fn from_env() -> envy::Result<Self> {
        envy::from_env()
    }
    pub fn duration(&self) -> Duration {
        Duration::from_secs(self.duration_sec)
    }
}

/// Configuration for the weights of loadtest operations
/// We use a random selection based on weight of operations. To perform some operations frequently, the developer must set the weight higher.
///
/// This configuration is independent from the main config for preserving simplicity of the main config
/// and do not break the backward compatibility
#[derive(Debug)]
pub struct ExecutionConfig {
    pub transaction_weights: TransactionWeights,
    pub contract_execution_params: LoadnextContractExecutionParams,
    pub explorer_api_config_weights: ExplorerApiRequestWeights,
}

impl ExecutionConfig {
    pub fn from_env() -> Self {
        let transaction_weights =
            TransactionWeights::from_env().unwrap_or_else(default_transaction_weights);
        let contract_execution_params = LoadnextContractExecutionParams::from_env()
            .unwrap_or_else(default_contract_execution_params);
        let explorer_api_config_weights = ExplorerApiRequestWeights::from_env()
            .unwrap_or_else(default_explorer_api_request_weights);
        Self {
            transaction_weights,
            contract_execution_params,
            explorer_api_config_weights,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExplorerApiRequestWeights {
    pub network_stats: f32,
    pub blocks: f32,
    pub block: f32,
    pub account_transactions: f32,
    pub transaction: f32,
    pub transactions: f32,
    pub account: f32,
    pub contract: f32,
    pub token: f32,
}

impl Default for ExplorerApiRequestWeights {
    fn default() -> Self {
        Self {
            network_stats: 1.0,
            blocks: 1.0,
            block: 1.0,
            transactions: 1.0,
            account: 1.0,
            token: 1.0,
            contract: 1.0,
            transaction: 1.0,
            account_transactions: 1.0,
        }
    }
}

impl ExplorerApiRequestWeights {
    pub fn from_env() -> Option<Self> {
        envy::prefixed("EXPLORER_API_REQUESTS_WEIGHTS_")
            .from_env()
            .ok()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionWeights {
    pub deposit: f32,
    pub withdrawal: f32,
    pub l1_transactions: f32,
    pub l2_transactions: f32,
}

impl TransactionWeights {
    pub fn from_env() -> Option<Self> {
        envy::prefixed("TRANSACTION_WEIGHTS_").from_env().ok()
    }
}

impl Default for TransactionWeights {
    fn default() -> Self {
        Self {
            deposit: 0.1,
            withdrawal: 0.5,
            l1_transactions: 0.1,
            l2_transactions: 1.0,
        }
    }
}

fn default_transaction_weights() -> TransactionWeights {
    let result = TransactionWeights::default();
    vlog::info!("Using default TransactionWeights: {:?}", &result);
    result
}

fn default_contract_execution_params() -> LoadnextContractExecutionParams {
    let result = LoadnextContractExecutionParams::default();
    vlog::info!(
        "Using default LoadnextContractExecutionParams: {:?}",
        &result
    );
    result
}

fn default_explorer_api_request_weights() -> ExplorerApiRequestWeights {
    let result = ExplorerApiRequestWeights::default();
    vlog::info!("Using default ExplorerApiRequestWeights: {:?}", &result);
    result
}
