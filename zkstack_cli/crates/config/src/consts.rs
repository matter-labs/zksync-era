/// Name of the main configuration file
pub(crate) const CONFIG_NAME: &str = "ZkStack.yaml";
/// Name of the wallets file
pub const WALLETS_FILE: &str = "wallets.yaml";
/// Name of the secrets config file
pub const SECRETS_FILE: &str = "secrets.yaml";
/// Name of the general config file
pub const GENERAL_FILE: &str = "general.yaml";
/// Name of the genesis config file
pub const GENESIS_FILE: &str = "genesis.yaml";

// Name of external node specific config
pub const EN_CONFIG_FILE: &str = "external_node.yaml";
// Name of consensus config
pub const CONSENSUS_CONFIG_FILE: &str = "consensus_config.yaml";
pub(crate) const ERC20_CONFIGS_FILE: &str = "erc20.yaml";
/// Name of the initial deployments config file
pub(crate) const INITIAL_DEPLOYMENT_FILE: &str = "initial_deployments.yaml";
/// Name of the erc20 deployments config file
pub(crate) const ERC20_DEPLOYMENT_FILE: &str = "erc20_deployments.yaml";
/// Name of the contracts file
pub const CONTRACTS_FILE: &str = "contracts.yaml";
/// Name of the gateway contracts file
pub const GATEWAY_FILE: &str = "gateway.yaml";
/// Name of the gateway contracts file
pub const GATEWAY_CHAIN_FILE: &str = "gateway_chain.yaml";
/// Main repository for the ZKsync project
pub const ZKSYNC_ERA_GIT_REPO: &str = "https://github.com/matter-labs/zksync-era";
/// Name of the docker-compose file inside zksync repository
pub const DOCKER_COMPOSE_FILE: &str = "docker-compose.yml";
/// Path to the config file with mnemonic for localhost wallets
pub const CONFIGS_PATH: &str = "etc/env/file_based";
/// Path to the docker-compose file for grafana
pub const ERA_OBSERVABILITY_COMPOSE_FILE: &str = "era-observability/docker-compose.yml";
/// Path to era observability repository
pub const ERA_OBSERBAVILITY_DIR: &str = "era-observability";
/// Era observability repo link
pub const ERA_OBSERBAVILITY_GIT_REPO: &str = "https://github.com/matter-labs/era-observability";
pub(crate) const LOCAL_APPS_PATH: &str = "apps/";
pub(crate) const LOCAL_CHAINS_PATH: &str = "chains/";
pub(crate) const LOCAL_CONFIGS_PATH: &str = "configs/";
pub(crate) const LOCAL_GENERATED_PATH: &str = ".generated/";
pub(crate) const LOCAL_DB_PATH: &str = "db/";
pub(crate) const LOCAL_ARTIFACTS_PATH: &str = "artifacts/";

/// Name of apps config file
pub const APPS_CONFIG_FILE: &str = "apps.yaml";
/// Name of portal runtime config file (auto-generated)
pub const PORTAL_JS_CONFIG_FILE: &str = "portal.config.js";
/// Name of portal config JSON file
pub const PORTAL_CONFIG_FILE: &str = "portal.config.json";
/// Name of explorer runtime config file (auto-generated)
pub const EXPLORER_JS_CONFIG_FILE: &str = "explorer.config.js";
/// Name of explorer config JSON file
pub const EXPLORER_CONFIG_FILE: &str = "explorer.config.json";
/// Name of explorer docker compose file
pub const EXPLORER_DOCKER_COMPOSE_FILE: &str = "explorer-docker-compose.yml";

pub const PRIVATE_RPC_DOCKER_COMPOSE_FILE: &str = "private-proxy-docker-compose.yml";

/// Default port for the explorer app
pub const DEFAULT_EXPLORER_PORT: u16 = 3010;
/// Default port for the portal app
pub const DEFAULT_PORTAL_PORT: u16 = 3030;
/// Default port for the explorer worker service
pub const DEFAULT_EXPLORER_WORKER_PORT: u16 = 3001;
/// Default port for the explorer API service
pub const DEFAULT_EXPLORER_API_PORT: u16 = 3002;
/// Default port for the explorer data fetcher service
pub const DEFAULT_EXPLORER_DATA_FETCHER_PORT: u16 = 3040;
/// Default session max age for prividium explorer backend
pub const DEFAULT_PRIVIDIUM_EXPLORER_SESSION_MAX_AGE: u64 = 86400000; // in milliseconds, 1 day
/// Default session same site for prividium explorer backend
pub const DEFAULT_PRIVIDIUM_EXPLORER_SESSION_SAME_SITE: &str = "strict";

/// Default port for the private rpc
pub const DEFAULT_PRIVATE_RPC_PORT: u16 = 4041;
/// Default token secret for the private rpc
pub const DEFAULT_PRIVATE_RPC_TOKEN_SECRET: &str = "sososecret";

/// Docker image for the explorer api
pub const EXPLORER_API_DOCKER_IMAGE: &str = "matterlabs/block-explorer-api";
/// Image tag for the explorer api
pub const EXPLORER_API_DOCKER_IMAGE_TAG: &str = "v2.73.1";
/// Image tag for the explorer api for prividium
pub const EXPLORER_API_PRIVIDIUM_DOCKER_IMAGE_TAG: &str = "v2.75.0-prividium.1";

/// Docker image for the explorer data fetcher
pub const EXPLORER_DATA_FETCHER_DOCKER_IMAGE: &str = "matterlabs/block-explorer-data-fetcher";
/// Image tag for the explorer data fetcher
pub const EXPLORER_DATA_FETCHER_DOCKER_IMAGE_TAG: &str = "v2.73.1";
/// Image tag for the explorer data fetcher for prividium
pub const EXPLORER_DATA_FETCHER_PRIVIDIUM_DOCKER_IMAGE_TAG: &str = "v2.75.0-prividium.1";

/// Docker image for the explorer worker
pub const EXPLORER_WORKER_DOCKER_IMAGE: &str = "matterlabs/block-explorer-worker";
/// Image tag for the explorer worker
pub const EXPLORER_WORKER_DOCKER_IMAGE_TAG: &str = "v2.73.1";
/// Image tag for the explorer worker for prividium
pub const EXPLORER_WORKER_PRIVIDIUM_DOCKER_IMAGE_TAG: &str = "v2.75.0-prividium.1";

/// Interval (in milliseconds) for polling new batches to process in explorer app
pub const EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL: u64 = 1000;

/// Path to ecosystem contacts
pub(crate) const ECOSYSTEM_PATH: &str = "etc/env/ecosystems";

/// Path to l1 contracts foundry folder inside zksync-era
pub(crate) const L1_CONTRACTS_FOUNDRY: &str = "contracts/l1-contracts";

pub(crate) const ERA_CHAIN_ID: u32 = 270;

pub(crate) const TEST_CONFIG_PATH: &str = "etc/test_config/constant/eth.json";
pub(crate) const BASE_PATH: &str = "m/44'/60'/0'";
