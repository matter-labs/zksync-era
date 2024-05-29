/// Ecosystem create related messages
pub(super) const MSG_L1_NETWORK_HELP: &str = "L1 Network";
pub(super) const MSG_LINK_TO_CODE_HELP: &str = "Code link";
pub(super) const MSG_START_CONTAINERS_HELP: &str =
    "Start reth and postgres containers after creation";
pub(super) const MSG_ECOSYSTEM_NAME_PROMPT: &str = "How do you want to name the ecosystem?";
pub(super) const MSG_REPOSITORY_ORIGIN_PROMPT: &str = "Select the origin of zksync-era repository";
pub(super) const MSG_LINK_TO_CODE_PROMPT: &str = "Where's the code located?";
pub(super) const MSG_L1_NETWORK_PROMPT: &str = "Select the L1 network";
pub(super) const MSG_START_CONTAINERS_PROMPT: &str =
    "Do you want to start containers after creating the ecosystem?";

/// Ecosystem init related messages
pub(super) const MSG_L1_RPC_URL_HELP: &str = "L1 RPC URL";
pub(super) const MSG_GENESIS_ARGS_HELP: &str = "Genesis options";
pub(super) const MSG_DEPLOY_ECOSYSTEM_PROMPT: &str =
    "Do you want to deploy ecosystem contracts? (Not needed if you already have an existing one)";
pub(super) const MSG_L1_RPC_URL_PROMPT: &str = "What is the RPC URL of the L1 network?";
pub(super) const MSG_DEPLOY_PAYMASTER_PROMPT: &str = "Do you want to deploy Paymaster contract?";
pub(super) const MSG_DEPLOY_ERC20_PROMPT: &str = "Do you want to deploy some test ERC20s?";
pub(super) const MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT: &str = "Provide the path to the ecosystem contracts or keep it empty and you will be added to ZkSync ecosystem";
pub(super) const MSG_L1_RPC_URL_INVALID_ERR: &str = "Invalid RPC URL";
pub(super) const MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR: &str = "Invalid path";
pub(super) const MSG_INITIALIZING_ECOSYSTEM: &str = "Initializing ecosystem";
pub(super) const MSG_DEPLOYING_ERC20: &str = "Deploying ERC20 contracts";
pub(super) const MSG_INITIALIZING_CHAIN: &str = "Initializing chain";
pub(super) const MSG_CHAIN_NOT_INITIALIZED: &str =
    "Chain not initialized. Please create a chain first";
pub(super) const MSG_ECOSYSTEM_INITIALIZED: &str = "Ecosystem initialized successfully with chains";
pub(super) const MSG_DISTRIBUTING_ETH_SPINNER: &str = "Distributing eth...";
pub(super) const MSG_INTALLING_DEPS_SPINNER: &str = "Installing and building dependencies...";
pub(super) const MSG_DEPLOYING_ERC20_SPINNER: &str = "Deploying ERC20 contracts...";
pub(super) const MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER: &str =
    "Deploying ecosystem contracts...";
