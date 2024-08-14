/// Name of the main configuration file
pub(crate) const CONFIG_NAME: &str = "ZkStack.yaml";
pub(crate) const PROVER_CONFIG_NAME: &str = "ProverSubsystem.yaml";
/// Name of the wallets file
pub const WALLETS_FILE: &str = "wallets.yaml";
/// Name of the secrets config file
pub const SECRETS_FILE: &str = "secrets.yaml";
/// Name of the general config file
pub const GENERAL_FILE: &str = "general.yaml";
/// Name of the genesis config file
pub const GENESIS_FILE: &str = "genesis.yaml";
pub const PROVER_FILE: &str = "prover.yaml";

// Name of external node specific config
pub const EN_CONFIG_FILE: &str = "external_node.yaml";
pub(crate) const ERC20_CONFIGS_FILE: &str = "erc20.yaml";
/// Name of the initial deployments config file
pub(crate) const INITIAL_DEPLOYMENT_FILE: &str = "initial_deployments.yaml";
/// Name of the erc20 deployments config file
pub(crate) const ERC20_DEPLOYMENT_FILE: &str = "erc20_deployments.yaml";
/// Name of the contracts file
pub const CONTRACTS_FILE: &str = "contracts.yaml";
/// Main repository for the ZKsync project
pub const ZKSYNC_ERA_GIT_REPO: &str = "https://github.com/matter-labs/zksync-era";
/// Name of the docker-compose file inside zksync repository
pub const DOCKER_COMPOSE_FILE: &str = "docker-compose.yml";
/// Path to the config file with mnemonic for localhost wallets
pub(crate) const CONFIGS_PATH: &str = "etc/env/file_based";
/// Path to the docker-compose file for grafana
pub const ERA_OBSERVABILITY_COMPOSE_FILE: &str = "era-observability/docker-compose.yml";
/// Path to era observability repository
pub const ERA_OBSERBAVILITY_DIR: &str = "era-observability";
/// Era observability repo link
pub const ERA_OBSERBAVILITY_GIT_REPO: &str = "https://github.com/matter-labs/era-observability";
pub(crate) const LOCAL_CONFIGS_PATH: &str = "configs/";
pub(crate) const LOCAL_DB_PATH: &str = "db/";

/// Path to ecosystem contacts
pub(crate) const ECOSYSTEM_PATH: &str = "etc/env/ecosystems";

/// Path to l1 contracts foundry folder inside zksync-era
pub(crate) const L1_CONTRACTS_FOUNDRY: &str = "contracts/l1-contracts";

pub(crate) const ERA_CHAIN_ID: u32 = 270;

pub(crate) const TEST_CONFIG_PATH: &str = "etc/test_config/constant/eth.json";
pub(crate) const BASE_PATH: &str = "m/44'/60'/0'";
