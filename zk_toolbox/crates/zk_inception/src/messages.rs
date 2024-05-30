/// Common messages
pub(super) const MSG_SELECTED_CONFIG: &str = "Selected config";
pub(super) const MSG_CHAIN_NOT_INITIALIZED: &str =
    "Chain not initialized. Please create a chain first";

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
pub(super) const MSG_CREATING_ECOSYSTEM: &str = "Creating ecosystem";
pub(super) const MSG_CREATED_ECOSYSTEM: &str = "Ecosystem created successfully";
pub(super) const MSG_CLONING_ERA_REPO_SPINNER: &str = "Cloning zksync-era repository...";
pub(super) const MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER: &str =
    "Creating initial configurations...";
pub(super) const MSG_CREATING_DEFAULT_CHAIN_SPINNER: &str = "Creating default chain...";
pub(super) const MSG_STARTING_CONTAINERS_SPINNER: &str = "Starting containers...";
pub(super) const MSG_ECOSYSTEM_ALREADY_EXISTS_ERR: &str = "Ecosystem already exists";
pub(super) const MSG_ECOSYSTEM_CONFIG_INVALID_ERR: &str = "Invalid ecosystem configuration";

/// Ecosystem and chain init related messages
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
pub(super) const MSG_GENESIS_DATABASE_ERR: &str = "Unable to perform genesis on the database";
pub(super) const MSG_CONTRACTS_CONFIG_NOT_FOUND_ERR: &str = "Ecosystem contracts config not found";
pub(super) const MSG_CHAIN_NOT_FOUND_ERR: &str = "Chain not found";
pub(super) const MSG_INITIALIZING_ECOSYSTEM: &str = "Initializing ecosystem";
pub(super) const MSG_DEPLOYING_ERC20: &str = "Deploying ERC20 contracts";
pub(super) const MSG_INITIALIZING_CHAIN: &str = "Initializing chain";
pub(super) const MSG_CHAIN_INITIALIZED: &str = "Chain initialized successfully";
pub(super) const MSG_ECOSYSTEM_INITIALIZED: &str = "Ecosystem initialized successfully with chains";
pub(super) const MSG_DISTRIBUTING_ETH_SPINNER: &str = "Distributing eth...";
pub(super) const MSG_INTALLING_DEPS_SPINNER: &str = "Installing and building dependencies...";
pub(super) const MSG_DEPLOYING_ERC20_SPINNER: &str = "Deploying ERC20 contracts...";
pub(super) const MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER: &str =
    "Deploying ecosystem contracts...";
pub(super) const MSG_REGISTERING_CHAIN_SPINNER: &str = "Registering chain...";
pub(super) const MSG_ACCEPTING_ADMIN_SPINNER: &str = "Accepting admin...";

/// Ecosystem default related messages
pub(super) const MSG_DEFAULT_CHAIN_PROMPT: &str = "What chain you want to set as default?";

/// Ecosystem config related messages
pub(super) const MSG_SAVE_INITIAL_CONFIG_ATTENTION: &str =
    "ATTENTION: This file contains sensible placeholders. Please check them and update with the desired values.";
pub(super) const MSG_SAVE_ERC20_CONFIG_ATTENTION: &str =
    "ATTENTION: This file should be filled with the desired ERC20 tokens to deploy.";

/// Chain create related messages
pub(super) const MSG_PROVER_MODE_HELP: &str = "Prover options";
pub(super) const MSG_WALLET_CREATION_HELP: &str = "Wallet options";
pub(super) const MSG_WALLET_PATH_HELP: &str = "Wallet path";
pub(super) const MSG_L1_COMMIT_DATA_GENERATOR_MODE_HELP: &str = "Commit data generation mode";
pub(super) const MSG_BASE_TOKEN_ADDRESS_HELP: &str = "Base token address";
pub(super) const MSG_BASE_TOKEN_PRICE_NOMINATOR_HELP: &str = "Base token nominator";
pub(super) const MSG_BASE_TOKEN_PRICE_DENOMINATOR_HELP: &str = "Base token denominator";
pub(super) const MSG_SET_AS_DEFAULT_HELP: &str = "Set as default chain";
pub(super) const MSG_CHAIN_NAME_PROMPT: &str = "What do you want to name the chain?";
pub(super) const MSG_CHAIN_ID_PROMPT: &str = "What's the chain id?";
pub(super) const MSG_WALLET_CREATION_PROMPT: &str = "Select how do you want to create the wallet";
pub(super) const MSG_PROVER_VERSION_PROMPT: &str = "Select the prover mode";
pub(super) const MSG_L1_BATCH_COMMIT_DATA_GENERATOR_MODE_PROMPT: &str =
    "Select the commit data generator mode";
pub(super) const MSG_WALLET_PATH_PROMPT: &str = "What is the wallet path?";
pub(super) const MSG_BASE_TOKEN_SELECTION_PROMPT: &str = "Select the base token to use";
pub(super) const MSG_BASE_TOKEN_ADDRESS_PROMPT: &str = "What is the token address?";
pub(super) const MSG_BASE_TOKEN_PRICE_NOMINATOR_PROMPT: &str =
    "What is the base token price nominator?";
pub(super) const MSG_BASE_TOKEN_PRICE_DENOMINATOR_PROMPT: &str =
    "What is the base token price denominator?";
pub(super) const MSG_SET_AS_DEFAULT_PROMPT: &str = "Set this chain as default?";
pub(super) const MSG_WALLET_PATH_INVALID_ERR: &str = "Invalid path";
pub(super) const MSG_NUMBER_VALIDATOR_NOT_ZERO_ERR: &str = "Number is not zero";
pub(super) const MSG_NUMBER_VALIDATOR_GREATHER_THAN_ZERO_ERR: &str =
    "Number should be greater than zero";
pub(super) const MSG_CREATING_CHAIN: &str = "Creating chain";
pub(super) const MSG_CHAIN_CREATED: &str = "Chain created successfully";
pub(super) const MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER: &str =
    "Creating chain configurations...";

/// Chain genesis related messages
pub(super) const MSG_SERVER_DB_URL_HELP: &str = "Server database url without database name";
pub(super) const MSG_SERVER_DB_NAME_HELP: &str = "Server database name";
pub(super) const MSG_PROVER_DB_URL_HELP: &str = "Prover database url without database name";
pub(super) const MSG_PROVER_DB_NAME_HELP: &str = "Prover database name";
pub(super) const MSG_GENESIS_USE_DEFAULT_HELP: &str = "Use default database urls and names";
pub(super) const MSG_SERVER_DB_URL_PROMPT: &str = "Please provide server database url for chain";
pub(super) const MSG_SERVER_DB_NAME_PROMPT: &str = "Please provide server database name for chain";
pub(super) const MSG_PROVER_DB_URL_PROMPT: &str = "Please provide prover database url for chain";
pub(super) const MSG_PROVER_DB_NAME_PROMPT: &str = "Please provide prover database name for chain";
pub(super) const MSG_GENESIS_COMPLETED: &str = "Genesis completed successfully";
pub(super) const MSG_GENESIS_DATABASE_CONFIG_ERR: &str = "Database config was not fully generated";
pub(super) const MSG_STARTING_GENESIS: &str = "Starting genesis process";
pub(super) const MSG_INITIALIZING_DATABASES_SPINNER: &str = "Initializing databases...";
pub(super) const MSG_STARTING_GENESIS_SPINNER: &str =
    "Starting the genesis of the server. Building the entire server may take a lot of time...";
pub(super) const MSG_INITIALIZING_SERVER_DATABASE: &str = "Initializing server database";
pub(super) const MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR: &str = "Failed to drop server database";
pub(super) const MSG_INITIALIZING_PROVER_DATABASE: &str = "Initializing prover database";
pub(super) const MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR: &str = "Failed to drop prover database";

/// Chain initialize bridges related messages
pub(super) const MSG_INITIALIZING_BRIDGES_SPINNER: &str = "Initializing bridges";

/// Chain deploy paymaster related messages
pub(super) const MSG_DEPLOYING_PAYMASTER: &str = "Deploying paymaster";
