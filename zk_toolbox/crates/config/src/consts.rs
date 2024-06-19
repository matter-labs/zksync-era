use types::ChainId;

/// Name of the main configuration file
pub(crate) const CONFIG_NAME: &str = "ZkStack.yaml";
/// Name of the wallets file
pub(crate) const WALLETS_FILE: &str = "wallets.yaml";
/// Name of the secrets config file
pub(crate) const SECRETS_FILE: &str = "secrets.yaml";
/// Name of the general config file
pub(crate) const GENERAL_FILE: &str = "general.yaml";
/// Name of the genesis config file
pub(crate) const GENESIS_FILE: &str = "genesis.yaml";

pub(crate) const ERC20_CONFIGS_FILE: &str = "erc20.yaml";
/// Name of the initial deployments config file
pub(crate) const INITIAL_DEPLOYMENT_FILE: &str = "initial_deployments.yaml";
/// Name of the erc20 deployments config file
pub(crate) const ERC20_DEPLOYMENT_FILE: &str = "erc20_deployments.yaml";
/// Name of the contracts file
pub(crate) const CONTRACTS_FILE: &str = "contracts.yaml";
/// Main repository for the ZKsync project
pub const ZKSYNC_ERA_GIT_REPO: &str = "https://github.com/matter-labs/zksync-era";
/// Name of the docker-compose file inside zksync repository
pub const DOCKER_COMPOSE_FILE: &str = "docker-compose.yml";
/// Path to the config file with mnemonic for localhost wallets
pub(crate) const CONFIGS_PATH: &str = "etc/env/file_based";
pub(crate) const LOCAL_CONFIGS_PATH: &str = "configs/";
pub(crate) const LOCAL_DB_PATH: &str = "db/";

/// Path to ecosystem contacts
pub(crate) const ECOSYSTEM_PATH: &str = "etc/ecosystem";

/// Path to l1 contracts foundry folder inside zksync-era
pub(crate) const L1_CONTRACTS_FOUNDRY: &str = "contracts/l1-contracts";

pub(crate) const ERA_CHAIN_ID: ChainId = ChainId(270);

pub(crate) const TEST_CONFIG_PATH: &str = "etc/test_config/constant/eth.json";
pub(crate) const BASE_PATH: &str = "m/44'/60'/0'";
