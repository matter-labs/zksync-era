use std::{
    fmt,
    path::{Display, Path},
    time::Duration,
};

use ethers::{
    types::{Address, H160, U256},
    utils::format_ether,
};
use url::Url;
use zksync_consensus_roles::validator;

use crate::utils::forge::WalletOwner;

pub(super) const MSG_SETUP_KEYS_DOWNLOAD_SELECTION_PROMPT: &str =
    "Do you want to download the setup keys or generate them?";
pub(super) const MSG_SETUP_KEYS_REGION_PROMPT: &str =
    "From which region you want setup keys to be downloaded?";
/// Common messages
pub(super) const MSG_SELECTED_CONFIG: &str = "Selected config";
pub(super) const MSG_CHAIN_NOT_INITIALIZED: &str =
    "Chain not initialized. Please create a chain first";
pub(super) const MSG_ARGS_VALIDATOR_ERR: &str = "Invalid arguments";
pub(super) const MSG_DEV_ARG_HELP: &str =
    "Use defaults for all options and flags. Suitable for local development";

/// Autocomplete message
pub(super) fn msg_generate_autocomplete_file(filename: &str) -> String {
    format!("Generating completion file: {filename}")
}
pub(super) const MSG_OUTRO_AUTOCOMPLETE_GENERATION: &str =
    "Autocompletion file correctly generated";

/// Ecosystem create related messages
pub(super) const MSG_L1_NETWORK_HELP: &str = "L1 Network";
pub(super) const MSG_LINK_TO_CODE_HELP: &str = "Code link";
pub(super) const MSG_START_CONTAINERS_HELP: &str =
    "Start reth and postgres containers after creation";
pub(super) const MSG_ECOSYSTEM_NAME_PROMPT: &str = "What do you want to name the ecosystem?";
pub(super) const MSG_REPOSITORY_ORIGIN_PROMPT: &str = "Select the origin of zksync-era repository";
pub(super) const MSG_LINK_TO_CODE_PROMPT: &str = "Where's the code located?";
pub(super) const MSG_L1_NETWORK_PROMPT: &str = "Select the L1 network";
pub(super) const MSG_START_CONTAINERS_PROMPT: &str =
    "Do you want to start containers after creating the ecosystem?";
pub(super) const MSG_CREATING_ECOSYSTEM: &str = "Creating ecosystem";

pub fn msg_created_ecosystem(name: &str) -> String {
    format!("Ecosystem {name} created successfully (All subsequent commands should be executed from ecosystem folder `cd {name}`)")
}

pub(super) const MSG_CLONING_ERA_REPO_SPINNER: &str = "Cloning zksync-era repository...";
pub(super) const MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER: &str =
    "Creating initial configurations...";
pub(super) const MSG_CREATING_DEFAULT_CHAIN_SPINNER: &str = "Creating default chain...";
pub(super) const MSG_STARTING_CONTAINERS_SPINNER: &str = "Starting containers...";
pub(super) const MSG_ECOSYSTEM_ALREADY_EXISTS_ERR: &str = "Ecosystem already exists";
pub(super) const MSG_ECOSYSTEM_CONFIG_INVALID_ERR: &str = "Invalid ecosystem configuration";
pub(super) const MSG_LINK_TO_CODE_SELECTION_CLONE: &str = "Clone for me (recommended)";
pub(super) const MSG_LINK_TO_CODE_SELECTION_PATH: &str = "I have the code already";
pub(super) const MSG_NOT_MAIN_REPO_OR_FORK_ERR: &str =
    "It's not a ZKsync Era main repository or fork";
pub(super) const MSG_CONFIRM_STILL_USE_FOLDER: &str = "Do you still want to use this folder?";

pub(super) fn msg_path_to_zksync_does_not_exist_err(path: &str) -> String {
    format!("Path to ZKsync Era repo does not exist: {path:?}")
}

/// Ecosystem and chain init related messages
pub(super) const MSG_L1_RPC_URL_HELP: &str = "L1 RPC URL";
pub(super) const MSG_NO_PORT_REALLOCATION_HELP: &str = "Do not reallocate ports";
pub(super) const MSG_GENESIS_ARGS_HELP: &str = "Genesis options";
pub(super) const MSG_OBSERVABILITY_HELP: &str = "Enable Grafana";
pub(super) const MSG_OBSERVABILITY_PROMPT: &str = "Do you want to setup observability? (Grafana)";
pub(super) const MSG_DEPLOY_ECOSYSTEM_PROMPT: &str =
    "Do you want to deploy ecosystem contracts? (Not needed if you already have an existing one)";
pub(super) const MSG_L1_RPC_URL_PROMPT: &str = "What is the RPC URL of the L1 network?";
pub(super) const MSG_DEPLOY_PAYMASTER_PROMPT: &str = "Do you want to deploy Paymaster contract?";
pub(super) const MSG_VALIDIUM_TYPE_PROMPT: &str = "Select the Validium type";
pub(super) const MSG_DEPLOY_ERC20_PROMPT: &str = "Do you want to deploy some test ERC20s?";
pub(super) const MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT: &str = "Provide the path to the ecosystem contracts or keep it empty and you will use ZKsync ecosystem config. \
For using this config, you need to have governance wallet";
pub(super) const MSG_L1_RPC_URL_INVALID_ERR: &str = "Invalid RPC URL";
pub(super) const MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR: &str = "Invalid path";
pub(super) const MSG_GENESIS_DATABASE_ERR: &str = "Unable to perform genesis on the database";
pub(super) const MSG_CHAIN_NOT_FOUND_ERR: &str = "Chain not found";
pub(super) const MSG_INITIALIZING_ECOSYSTEM: &str = "Initializing ecosystem";
pub(super) const MSG_DEPLOYING_ERC20: &str = "Deploying ERC20 contracts";
pub(super) const MSG_CHAIN_INITIALIZED: &str = "Chain initialized successfully";
pub(super) const MSG_CHAIN_CONFIGS_INITIALIZED: &str = "Chain configs were initialized";
pub(super) const MSG_CHAIN_OWNERSHIP_TRANSFERRED: &str =
    "Chain ownership was transferred successfully";
pub(super) const MSG_EVM_EMULATOR_ENABLED: &str = "EVM emulator enabled successfully";
pub(super) const MSG_CHAIN_REGISTERED: &str = "Chain registraion was successful";
pub(super) const MSG_DISTRIBUTING_ETH_SPINNER: &str = "Distributing eth...";
pub(super) const MSG_MINT_BASE_TOKEN_SPINNER: &str =
    "Minting base token to the governance addresses...";
pub(super) const MSG_INTALLING_DEPS_SPINNER: &str = "Installing and building dependencies...";
pub(super) const MSG_PREPARING_CONFIG_SPINNER: &str = "Preparing config files...";
pub(super) const MSG_DEPLOYING_ERC20_SPINNER: &str = "Deploying ERC20 contracts...";
pub(super) const MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER: &str =
    "Deploying ecosystem contracts...";
pub(super) const MSG_REGISTERING_CHAIN_SPINNER: &str = "Registering chain...";
pub(super) const MSG_ACCEPTING_ADMIN_SPINNER: &str = "Accepting admin...";
pub(super) const MSG_DA_PAIR_REGISTRATION_SPINNER: &str = "Registering DA pair...";
pub(super) const MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER: &str =
    "Updating token multiplier setter...";
pub(super) const MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER: &str = "Updating da validator pair...";
pub(super) const MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO: &str =
    "Token multiplier setter updated to";
pub(super) const MSG_DA_VALIDATOR_PAIR_UPDATED_TO: &str = "DA validator pair updated to";
pub(super) const MSG_GOT_SETTLEMENT_LAYER_ADDRESS_FROM_GW: &str =
    "Got the settlement layer address from gateway";
pub(super) const MSG_UPDATING_PUBDATA_PRICING_MODE_SPINNER: &str =
    "Updating pubdata pricing mode...";
pub(super) const MSG_PUBDATA_PRICING_MODE_UPDATED_TO: &str = "Pubdata pricing mode updated to";
pub(super) const MSG_RECREATE_ROCKS_DB_ERRROR: &str = "Failed to create rocks db path";
pub(super) const MSG_ERA_OBSERVABILITY_ALREADY_SETUP: &str = "Era observability already setup";
pub(super) const MSG_DOWNLOADING_ERA_OBSERVABILITY_SPINNER: &str =
    "Downloading era observability...";

pub(super) fn msg_ecosystem_no_found_preexisting_contract(chains: &str) -> String {
    format!("Not found preexisting ecosystem Contracts with chains {chains}")
}

pub(super) fn msg_initializing_chain(chain_name: &str) -> String {
    format!("Initializing chain {chain_name}")
}

pub(super) fn msg_ecosystem_initialized(chains: &str) -> String {
    if chains.is_empty() {
        "Ecosystem initialized successfully. You can initialize chain with `chain init`".to_string()
    } else {
        format!("Ecosystem initialized successfully with chains {chains}")
    }
}

/// Ecosystem default related messages
pub(super) const MSG_DEFAULT_CHAIN_PROMPT: &str = "What chain do you want to set as default?";

/// Ecosystem config related messages
pub(super) const MSG_SAVE_INITIAL_CONFIG_ATTENTION: &str =
    "ATTENTION: This file contains sensible placeholders. Please check them and update with the desired values.";
pub(super) const MSG_SAVE_ERC20_CONFIG_ATTENTION: &str =
    "ATTENTION: This file should be filled with the desired ERC20 tokens to deploy.";

/// Ecosystem change default related messages
pub(super) fn msg_chain_doesnt_exist_err(chain_name: &str, chains: &Vec<String>) -> String {
    format!(
        "Chain with name {} doesnt exist, please choose one of {:?}",
        chain_name, chains
    )
}
pub(super) fn msg_chain_load_err(chain_name: &str) -> String {
    format!("Failed to load chain config for {chain_name}")
}

/// Build ecosystem transactions related messages
pub(super) const MSG_SENDER_ADDRESS_PROMPT: &str = "What is the address of the transaction sender?";
pub(super) const MSG_BUILDING_ECOSYSTEM: &str = "Building ecosystem transactions";
pub(super) const MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER: &str = "Building ecosystem contracts...";
pub(super) const MSG_WRITING_OUTPUT_FILES_SPINNER: &str = "Writing output files...";
pub(super) const MSG_ECOSYSTEM_TXN_OUTRO: &str = "Transactions successfully built";
pub(super) const MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR: &str = "Invalid path";

/// Chain create related messages
pub(super) const MSG_PROVER_MODE_HELP: &str = "Prover options";
pub(super) const MSG_CHAIN_ID_HELP: &str = "Chain ID";
pub(super) const MSG_WALLET_CREATION_HELP: &str = "Wallet options";
pub(super) const MSG_WALLET_PATH_HELP: &str = "Wallet path";
pub(super) const MSG_L1_COMMIT_DATA_GENERATOR_MODE_HELP: &str = "Commit data generation mode";
pub(super) const MSG_BASE_TOKEN_ADDRESS_HELP: &str = "Base token address";
pub(super) const MSG_BASE_TOKEN_PRICE_NOMINATOR_HELP: &str = "Base token nominator";
pub(super) const MSG_BASE_TOKEN_PRICE_DENOMINATOR_HELP: &str = "Base token denominator";
pub(super) const MSG_SET_AS_DEFAULT_HELP: &str = "Set as default chain";
pub(super) const MSG_EVM_EMULATOR_HELP: &str = "Enable EVM emulator";
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
pub(super) const MSG_EVM_EMULATOR_PROMPT: &str = "Enable EVM emulator?";
pub(super) const MSG_WALLET_PATH_INVALID_ERR: &str = "Invalid path";
pub(super) const MSG_NUMBER_VALIDATOR_NOT_ZERO_ERR: &str = "Number is not zero";
pub(super) const MSG_NUMBER_VALIDATOR_GREATHER_THAN_ZERO_ERR: &str =
    "Number should be greater than zero";
pub(super) const MSG_CREATING_CHAIN: &str = "Creating chain";
pub(super) const MSG_CHAIN_CREATED: &str = "Chain created successfully";
pub(super) const MSG_CREATING_CHAIN_CONFIGURATIONS_SPINNER: &str =
    "Creating chain configurations...";
pub(super) const MSG_CHAIN_ID_VALIDATOR_ERR: &str = "Invalid chain id";
pub(super) const MSG_BASE_TOKEN_ADDRESS_VALIDATOR_ERR: &str = "Invalid base token address";
pub(super) const MSG_WALLET_CREATION_VALIDATOR_ERR: &str =
    "Localhost wallet is not supported for external networks";
pub(super) const MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND: &str =
    "Token Multiplier Setter not found. Specify it in a wallet config";
pub(super) const MSG_EVM_EMULATOR_HASH_MISSING_ERR: &str =
    "Impossible to initialize a chain with EVM emulator: the template genesis config \
     does not contain EVM emulator hash";

/// Chain genesis related messages
pub(super) const MSG_SERVER_DB_URL_HELP: &str = "Server database url without database name";
pub(super) const MSG_SERVER_DB_NAME_HELP: &str = "Server database name";
pub(super) const MSG_PROVER_DB_URL_HELP: &str = "Prover database url without database name";
pub(super) const MSG_PROVER_DB_NAME_HELP: &str = "Prover database name";
pub(super) const MSG_SERVER_COMMAND_HELP: &str = "Command to run the server binary";
pub(super) const MSG_USE_DEFAULT_DATABASES_HELP: &str = "Use default database urls and names";
pub(super) const MSG_GENESIS_COMPLETED: &str = "Genesis completed successfully";
pub(super) const MSG_STARTING_GENESIS: &str = "Starting genesis process";
pub(super) const MSG_INITIALIZING_DATABASES_SPINNER: &str = "Initializing databases...";
pub(super) const MSG_STARTING_GENESIS_SPINNER: &str =
    "Starting the genesis of the server. Building the entire server may take a lot of time...";
pub(super) const MSG_INITIALIZING_SERVER_DATABASE: &str = "Initializing server database";
pub(super) const MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR: &str = "Failed to drop server database";
pub(super) const MSG_INITIALIZING_PROVER_DATABASE: &str = "Initializing prover database";
pub(super) const MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR: &str = "Failed to drop prover database";
pub(super) const MSG_GENESIS_DATABASES_INITIALIZED: &str = "Databases initialized successfully";

/// Chain update related messages
pub(super) const MSG_WALLETS_CONFIG_MUST_BE_PRESENT: &str = "Wallets configuration must be present";

pub(super) fn msg_server_db_url_prompt(chain_name: &str) -> String {
    format!("Please provide server database url for chain {chain_name}")
}

pub(super) fn msg_external_node_db_url_prompt(chain_name: &str) -> String {
    format!("Please provide external_node database url for chain {chain_name}")
}

pub(super) fn msg_prover_db_url_prompt(chain_name: &str) -> String {
    format!("Please provide prover database url for chain {chain_name}")
}

pub(super) fn msg_prover_db_name_prompt(chain_name: &str) -> String {
    format!("Please provide prover database name for chain {chain_name}")
}

pub(super) fn msg_external_node_db_name_prompt(chain_name: &str) -> String {
    format!("Please provide external_node database name for chain {chain_name}")
}

pub(super) fn msg_server_db_name_prompt(chain_name: &str) -> String {
    format!("Please provide server database name for chain {chain_name}")
}

pub(super) fn msg_explorer_db_url_prompt(chain_name: &str) -> String {
    format!("Please provide explorer database url for chain {chain_name}")
}

pub(super) fn msg_explorer_db_name_prompt(chain_name: &str) -> String {
    format!("Please provide explorer database name for chain {chain_name}")
}

/// Chain initialize bridges related messages
pub(super) const MSG_DEPLOYING_L2_CONTRACT_SPINNER: &str = "Deploying l2 contracts";

/// Chain deploy paymaster related messages
pub(super) const MSG_DEPLOYING_PAYMASTER: &str = "Deploying paymaster";

/// Chain build related messages
pub(super) const MSG_BUILDING_CHAIN_REGISTRATION_TXNS_SPINNER: &str =
    "Building chain registration transactions...";
pub(super) const MSG_CHAIN_TXN_OUT_PATH_INVALID_ERR: &str = "Invalid path";
pub(super) const MSG_CHAIN_TXN_MISSING_CONTRACT_CONFIG: &str =
    "Missing contract.yaml, please be sure to run this command within initialized ecosystem";
pub(super) const MSG_CHAIN_TRANSACTIONS_BUILT: &str = "Chain transactions successfully built";

/// Run server related messages
pub(super) const MSG_SERVER_COMPONENTS_HELP: &str = "Components of server to run";
pub(super) const MSG_ENABLE_CONSENSUS_HELP: &str = "Enable consensus";
pub(super) const MSG_SERVER_GENESIS_HELP: &str = "Run server in genesis mode";
pub(super) const MSG_SERVER_ADDITIONAL_ARGS_HELP: &str =
    "Additional arguments that can be passed through the CLI";
pub(super) const MSG_SERVER_URING_HELP: &str = "Enables uring support for RocksDB";

/// Accept ownership related messages
pub(super) const MSG_ACCEPTING_GOVERNANCE_SPINNER: &str = "Accepting governance...";

/// EVM emulator related messages
pub(super) const MSG_ENABLING_EVM_EMULATOR: &str = "Enabling EVM emulator...";

/// Containers related messages
pub(super) const MSG_STARTING_CONTAINERS: &str = "Starting containers";
pub(super) const MSG_STARTING_DOCKER_CONTAINERS_SPINNER: &str =
    "Starting containers using docker...";
pub(super) const MSG_CONTAINERS_STARTED: &str = "Containers started successfully";
pub(super) const MSG_RETRY_START_CONTAINERS_PROMPT: &str =
    "Failed to start containers. Make sure there is nothing running on default ports for Ethereum node l1 and postgres. Want to try again?";
pub(super) const MSG_FAILED_TO_FIND_ECOSYSTEM_ERR: &str = "Failed to find ecosystem folder.";
pub(super) const MSG_OBSERVABILITY_RUN_PROMPT: &str = "Do you want to run observability?";

/// Server related messages
pub(super) const MSG_STARTING_SERVER: &str = "Starting server";
pub(super) const MSG_FAILED_TO_RUN_SERVER_ERR: &str = "Failed to start server";
pub(super) const MSG_PREPARING_EN_CONFIGS: &str = "Preparing External Node config";
pub(super) const MSG_BUILDING_SERVER: &str = "Building server";
pub(super) const MSG_FAILED_TO_BUILD_SERVER_ERR: &str = "Failed to build server";
pub(super) const MSG_WAITING_FOR_SERVER: &str = "Waiting for server to start";

pub(super) fn msg_waiting_for_server_success(health_check_url: &str) -> String {
    format!("Server is alive with health check server on {health_check_url}")
}

/// Portal related messages
pub(super) const MSG_PORTAL_FAILED_TO_FIND_ANY_CHAIN_ERR: &str =
    "Failed to find any valid chain to run portal for";
pub(super) const MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR: &str = "Failed to create portal config";
pub(super) const MSG_PORTAL_FAILED_TO_RUN_DOCKER_ERR: &str =
    "Failed to run portal docker container";
pub(super) fn msg_portal_running_with_config(path: &Path) -> String {
    format!("Running portal with configuration from: {}", path.display())
}
pub(super) fn msg_portal_starting_on(host: &str, port: u16) -> String {
    format!("Starting portal on http://{host}:{port}")
}

/// Private proxy related messages
pub(super) const MSG_PRIVATE_RPC_FAILED_TO_RUN_DOCKER_ERR: &str =
    "Failed to run private proxy container";

pub(super) fn msg_private_rpc_db_url_prompt(chain_name: &str) -> String {
    format!("Please provide private proxy database url for chain {chain_name}")
}

pub(super) fn msg_private_rpc_initializing_database_for(chain: &str) -> String {
    format!("Initializing private proxy database for {chain} chain")
}

pub(super) fn msg_private_rpc_docker_image_being_built() -> String {
    "Building private-proxy docker image, it may take a while...".to_string()
}
pub(super) fn msg_private_rpc_docker_compose_file_generated(path: Display) -> String {
    format!("Generated private proxy docker-compose file and stored it at {path}")
}
pub(super) fn msg_private_rpc_permissions_file_generated(path: Display) -> String {
    format!("Created example permissions config and stored it at {path}")
}

pub(super) fn msg_private_rpc_chain_not_initialized(chain: &str) -> String {
    format!("Chain {chain} is not initialized for private-proxy: run `zkstack private-proxy init --chain {chain}` first")
}

pub(super) fn msg_private_proxy_db_name_prompt(chain_name: &str) -> String {
    format!("Please provide private proxy database name for chain {chain_name}")
}

pub(super) const MSG_PRIVATE_RPC_FAILED_TO_DROP_DATABASE_ERR: &str =
    "Failed to drop private proxy database";

/// Explorer related messages
pub(super) const MSG_EXPLORER_PRIVIDIUM_HELP: &str =
    "Enable Prividium mode for this Block Explorer";
pub(super) const MSG_EXPLORER_FAILED_TO_DROP_DATABASE_ERR: &str =
    "Failed to drop explorer database";
pub(super) const MSG_EXPLORER_FAILED_TO_RUN_DOCKER_SERVICES_ERR: &str =
    "Failed to run docker compose with explorer services";
pub(super) const MSG_EXPLORER_FAILED_TO_RUN_DOCKER_ERR: &str =
    "Failed to run explorer docker container";
pub(super) const MSG_EXPLORER_FAILED_TO_CREATE_CONFIG_ERR: &str =
    "Failed to create explorer config";
pub(super) const MSG_EXPLORER_FAILED_TO_FIND_ANY_CHAIN_ERR: &str =
    "Failed to find any valid chain to run explorer for. Did you run `zkstack explorer init`?";
pub(super) const MSG_EXPLORER_INITIALIZED: &str = "Explorer has been initialized successfully";
pub(super) fn msg_explorer_initializing_database_for(chain: &str) -> String {
    format!("Initializing explorer database for {chain} chain")
}
pub(super) fn msg_explorer_running_with_config(path: &Path) -> String {
    format!(
        "Running explorer with configuration from: {}",
        path.display()
    )
}
pub(super) fn msg_explorer_starting_on(host: &str, port: u16) -> String {
    format!("Starting explorer on http://{host}:{port}")
}
pub(super) fn msg_explorer_chain_not_initialized(chain: &str) -> String {
    format!("Chain {chain} is not initialized for explorer: run `zkstack explorer init --chain {chain}` first")
}

pub(super) const MSG_EXPLORER_PRIVIDIUM_MODE_PROMPT: &str =
    "Do you want to enable Prividium mode for this Block Explorer?";

pub(super) const MSG_EXPLORER_PRIVIDIUM_SESSION_MAX_AGE_PROMPT: &str =
    "What session max age configuration do you want to use for Prividium mode?";

pub(super) const MSG_EXPLORER_PRIVIDIUM_SESSION_SAME_SITE_PROMPT: &str =
    "What session same site configuration do you want to use for Prividium mode?";

/// Forge utils related messages
pub(super) fn msg_wallet_private_key_not_set(wallet_owner: WalletOwner) -> String {
    format!(
        "{} private key is not set",
        match wallet_owner {
            WalletOwner::Governor => "Governor",
            WalletOwner::Deployer => "Deployer",
        }
    )
}

pub(super) fn msg_address_doesnt_have_enough_money_prompt(
    address: &H160,
    actual: U256,
    expected: U256,
) -> String {
    let actual = format_ether(actual);
    let expected = format_ether(expected);
    format!(
        "It is recommended to have {expected} ETH on the address {address:?} to deploy contracts. Current balance is {actual} ETH. How do you want to proceed?",
    )
}

pub(super) fn msg_preparing_en_config_is_done(path: &Path) -> String {
    format!("External nodes configs could be found in: {path:?}")
}

pub(super) const MSG_EXTERNAL_NODE_CONFIG_NOT_INITIALIZED: &str =
    "External node is not initialized";

pub(super) const MSG_BUILDING_EN: &str = "Building external node";
pub(super) const MSG_FAILED_TO_BUILD_EN_ERR: &str = "Failed to build external node";
pub(super) const MSG_STARTING_EN: &str = "Starting external node";
pub(super) const MSG_WAITING_FOR_EN: &str = "Waiting for external node to start";

pub(super) fn msg_waiting_for_en_success(health_check_url: &str) -> String {
    format!("External node is alive with health check server on {health_check_url}")
}

/// Prover related messages
pub(super) const MSG_GENERATING_SK_SPINNER: &str = "Generating setup keys...";
pub(super) const MSG_SK_GENERATED: &str = "Setup keys generated successfully";
pub(super) const MSG_MISSING_COMPONENT_ERR: &str = "Missing component";
pub(super) const MSG_RUNNING_PROVER_GATEWAY: &str = "Running gateway";
pub(super) const MSG_RUNNING_PROVER_JOB_MONITOR_ERR: &str = "Failed to run prover job monitor";
pub(super) const MSG_RUNNING_PROVER_JOB_MONITOR: &str = "Running prover job monitor";
pub(super) const MSG_RUNNING_WITNESS_GENERATOR: &str = "Running witness generator";
pub(super) const MSG_RUNNING_CIRCUIT_PROVER: &str = "Running circuit prover";
pub(super) const MSG_RUNNING_COMPRESSOR: &str = "Running compressor";
pub(super) const MSG_RUN_COMPONENT_PROMPT: &str = "What component do you want to run?";
pub(super) const MSG_RUNNING_PROVER_GATEWAY_ERR: &str = "Failed to run prover gateway";
pub(super) const MSG_RUNNING_WITNESS_GENERATOR_ERR: &str = "Failed to run witness generator";
pub(super) const MSG_RUNNING_COMPRESSOR_ERR: &str = "Failed to run compressor";
pub(super) const MSG_RUNNING_CIRCUIT_PROVER_ERR: &str = "Failed to run circuit prover";
pub(super) const MSG_PROOF_STORE_CONFIG_PROMPT: &str =
    "Select where you would like to store the proofs";
pub(super) const MSG_PROOF_STORE_DIR_PROMPT: &str =
    "Provide the path where you would like to store the proofs:";
pub(super) const MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_PROMPT: &str =
    "Provide the base URL of the GCS bucket (e.g., gs://bucket-name):";
pub(super) const MSG_PROOF_STORE_GCS_BUCKET_BASE_URL_ERR: &str =
    "Bucket base URL should start with gs://";
pub(super) const MSG_PROOF_STORE_GCS_CREDENTIALS_FILE_PROMPT: &str =
    "Provide the path to the GCS credentials file:";
pub(super) const MSG_PROVER_INITIALIZED: &str = "Prover has been initialized successfully";
pub(super) const MSG_CREATE_GCS_BUCKET_PROMPT: &str = "Do you want to create a new GCS bucket?";
pub(super) const MSG_CREATE_GCS_BUCKET_PROJECT_ID_PROMPT: &str = "Select the project ID:";
pub(super) const MSG_CREATE_GCS_BUCKET_PROJECT_ID_NO_PROJECTS_PROMPT: &str =
    "Provide a project ID:";
pub(super) const MSG_CREATE_GCS_BUCKET_NAME_PROMTP: &str = "What do you want to name the bucket?";
pub(super) const MSG_CREATE_GCS_BUCKET_LOCATION_PROMPT: &str = "What location do you want to use? Find available locations at https://cloud.google.com/storage/docs/locations";
pub(super) const MSG_DOWNLOADING_SETUP_COMPRESSOR_KEY_SPINNER: &str =
    "Downloading compressor setup key...";
pub(super) const MSG_DOWNLOAD_SETUP_COMPRESSOR_KEY_PROMPT: &str =
    "Do you want to download the setup key for compressor?";
pub(super) const MSG_INITIALIZE_BELLMAN_CUDA_PROMPT: &str =
    "Do you want to initialize bellman-cuda?";
pub(super) const MSG_SETUP_COMPRESSOR_KEY_PATH_PROMPT: &str = "Provide the path to the setup key:";
pub(super) const MSG_GETTING_GCP_PROJECTS_SPINNER: &str = "Getting GCP projects...";
pub(super) const MSG_GETTING_PROOF_STORE_CONFIG: &str = "Getting proof store configuration...";
pub(super) const MSG_CREATING_GCS_BUCKET_SPINNER: &str = "Creating GCS bucket...";
pub(super) const MSG_ROUND_SELECT_PROMPT: &str = "Select the round to run";
pub(super) const MSG_WITNESS_GENERATOR_ROUND_ERR: &str = "Witness generator round not found";
pub(super) const MSG_SETUP_KEY_PATH_ERROR: &str = "Failed to get setup key path";
pub(super) const MSG_CLONING_BELLMAN_CUDA_SPINNER: &str = "Cloning bellman-cuda...";
pub(super) const MSG_BUILDING_BELLMAN_CUDA_SPINNER: &str = "Building bellman-cuda...";
pub(super) const MSG_BELLMAN_CUDA_DIR_ERR: &str = "Failed to get bellman-cuda directory";
pub(super) const MSG_BELLMAN_CUDA_DIR_PROMPT: &str =
    "Provide the path to the bellman-cuda directory:";
pub(super) const MSG_BELLMAN_CUDA_INITIALIZED: &str =
    "bellman-cuda has been initialized successfully";
pub(super) const MSG_BELLMAN_CUDA_ORIGIN_SELECT: &str =
    "Select the origin of bellman-cuda repository";
pub(super) const MSG_BELLMAN_CUDA_SELECTION_CLONE: &str = "Clone for me (recommended)";
pub(super) const MSG_BELLMAN_CUDA_SELECTION_PATH: &str = "I have the code already";
pub(super) const MSG_SETUP_KEYS_PROMPT: &str = "Do you want to setup keys?";

pub(super) fn msg_bucket_created(bucket_name: &str) -> String {
    format!("Bucket created successfully with url: gs://{bucket_name}")
}

/// Contract verifier related messages
pub(super) const MSG_BUILDING_CONTRACT_VERIFIER: &str = "Building contract verifier";
pub(super) const MSG_RUNNING_CONTRACT_VERIFIER: &str = "Running contract verifier";
pub(super) const MSG_FAILED_TO_BUILD_CONTRACT_VERIFIER_ERR: &str =
    "Failed to build contract verifier";
pub(super) const MSG_FAILED_TO_RUN_CONTRACT_VERIFIER_ERR: &str = "Failed to run contract verifier";
pub(super) const MSG_INVALID_ARCH_ERR: &str = "Invalid arch";
pub(super) const MSG_GET_ZKSOLC_RELEASES_ERR: &str = "Failed to get zksolc releases";
pub(super) const MSG_FETCHING_ZKSOLC_RELEASES_SPINNER: &str = "Fetching zksolc releases...";
pub(super) const MSG_FETCHING_ZKVYPER_RELEASES_SPINNER: &str = "Fetching zkvyper releases...";
pub(super) const MSG_FETCH_SOLC_RELEASES_SPINNER: &str = "Fetching solc releases...";
pub(super) const MSG_FETCH_ERA_VM_SOLC_RELEASES_SPINNER: &str = "Fetching era vm solc releases...";
pub(super) const MSG_FETCHING_VYPER_RELEASES_SPINNER: &str = "Fetching vyper releases...";
pub(super) const MSG_ZKSOLC_VERSION_PROMPT: &str = "Select the minimal zksolc version:";
pub(super) const MSG_ZKVYPER_VERSION_PROMPT: &str = "Select the minimal zkvyper version:";
pub(super) const MSG_SOLC_VERSION_PROMPT: &str = "Select the minimal solc version:";
pub(super) const MSG_ERA_VM_SOLC_VERSION_PROMPT: &str = "Select the minimal era vm solc version:";
pub(super) const MSG_VYPER_VERSION_PROMPT: &str = "Select the minimal vyper version:";
pub(super) const MSG_NO_RELEASES_FOUND_ERR: &str = "No releases found for current architecture";
pub(super) const MSG_NO_VERSION_FOUND_ERR: &str = "No version found";
pub(super) const MSG_ARCH_NOT_SUPPORTED_ERR: &str = "Architecture not supported";
pub(super) const MSG_OS_NOT_SUPPORTED_ERR: &str = "OS not supported";
pub(super) const MSG_GET_VYPER_RELEASES_ERR: &str = "Failed to get vyper releases";
pub(super) const MSG_GET_SOLC_RELEASES_ERR: &str = "Failed to get solc releases";
pub(super) const MSG_GET_ERA_VM_SOLC_RELEASES_ERR: &str = "Failed to get era vm solc releases";
pub(super) const MSG_GET_ZKVYPER_RELEASES_ERR: &str = "Failed to get zkvyper releases";

pub(super) fn msg_binary_already_exists(name: &str, version: &str) -> String {
    format!(
        "{} {} binary already exists. Skipping download.",
        name, version
    )
}

pub(super) fn msg_downloading_binary_spinner(name: &str, version: &str) -> String {
    format!("Downloading {} {} binary", name, version)
}

// Update related messages

pub(super) const MSG_UPDATE_ONLY_CONFIG_HELP: &str = "Update only the config files";
pub(super) const MSG_UPDATING_ZKSYNC: &str = "Updating ZKsync";
pub(super) const MSG_ZKSYNC_UPDATED: &str = "ZKsync updated successfully";
pub(super) const MSG_PULLING_ZKSYNC_CODE_SPINNER: &str = "Pulling zksync-era repo...";
pub(super) const MSG_UPDATING_SUBMODULES_SPINNER: &str = "Updating submodules...";
pub(super) const MSG_DIFF_GENERAL_CONFIG: &str =
    "Added the following fields to the general config:";
pub(super) const MSG_DIFF_EN_CONFIG: &str =
    "Added the following fields to the external node config:";
pub(super) const MSG_DIFF_EN_GENERAL_CONFIG: &str =
    "Added the following fields to the external node generalconfig:";
pub(super) const MSG_UPDATING_ERA_OBSERVABILITY_SPINNER: &str = "Updating era observability...";

/// Wait-related messages
pub(super) const MSG_WAIT_TIMEOUT_HELP: &str = "Wait timeout in seconds";
pub(super) const MSG_WAIT_POLL_INTERVAL_HELP: &str = "Poll interval in milliseconds";

pub(super) fn msg_wait_starting_polling(
    component: &impl fmt::Display,
    url: &str,
    poll_interval: Duration,
) -> String {
    format!("Starting polling {component} at `{url}` each {poll_interval:?}")
}

pub(super) fn msg_wait_timeout(component: &impl fmt::Display) -> String {
    format!("timed out polling {component}")
}

pub(super) fn msg_wait_non_successful_response(component: &impl fmt::Display) -> String {
    format!("non-successful {component} response")
}

pub(super) fn msg_wait_not_healthy(url: &str) -> String {
    format!("Node at `{url}` is not healthy")
}

pub(super) fn msg_diff_genesis_config(chain: &str) -> String {
    format!(
        "Found differences between chain {chain} and era genesis configs. Consider updating the chain {chain} genesis config and re-running genesis. Diff:"
    )
}

pub(super) fn msg_diff_contracts_config(chain: &str) -> String {
    format!(
        "Found differences between chain {chain} and era contracts configs. Consider updating the chain {chain} contracts config and re-running genesis. Diff:"
    )
}

pub(super) fn msg_diff_secrets(
    chain: &str,
    current_secrets_path: &Path,
    era_secret_path: &Path,
) -> String {
    format!(
        "Found differences between chain {chain} and era secrets configs. Consider updating the chain {chain} secrets config at {current_secrets_path:?} using the file {era_secret_path:?} as reference. Diff:"
    )
}

pub(super) fn msg_updating_chain(chain: &str) -> String {
    format!("Updating chain: {}", chain)
}

/// consensus command messages
pub(super) const MSG_RECEIPT_MISSING: &str = "receipt missing";
pub(super) const MSG_STATUS_MISSING: &str = "status missing";
pub(super) const MSG_TRANSACTION_FAILED: &str = "transaction failed";
pub(super) const MSG_MULTICALL3_CONTRACT_NOT_CONFIGURED: &str =
    "multicall3 contract not configured";
pub(super) const MSG_GOVERNOR_PRIVATE_KEY_NOT_SET: &str = "governor private key not set";
pub(super) const MSG_CONSENSUS_REGISTRY_ADDRESS_NOT_CONFIGURED: &str =
    "consensus registry address not configured";
pub(super) const MSG_CONSENSUS_REGISTRY_POLL_ERROR: &str = "failed querying L2 node";
pub(super) const MSG_CONSENSUS_REGISTRY_WAIT_COMPONENT: &str = "main node HTTP RPC";

pub(super) fn msg_setting_validator_committee_failed(
    got: &validator::Committee,
    want: &validator::Committee,
) -> String {
    format!("setting validator committee failed: got {got:?}, want {want:?}")
}

pub(super) fn msg_wait_consensus_registry_started_polling(addr: Address, url: &Url) -> String {
    format!("Starting polling L2 HTTP RPC at {url} for code at {addr:?}")
}

pub(super) fn msg_consensus_registry_wait_success(addr: Address, code_len: usize) -> String {
    format!("Consensus registry is deployed at {addr:?}: {code_len} bytes")
}

/// DA clients related messages
pub(super) const MSG_AVAIL_CLIENT_TYPE_PROMPT: &str = "Avail client type";
pub(super) const MSG_AVAIL_API_TIMEOUT_MS: &str = "Avail API timeout in milliseconds";
pub(super) const MSG_AVAIL_API_NODE_URL_PROMPT: &str = "Avail API node URL";
pub(super) const MSG_AVAIL_APP_ID_PROMPT: &str = "Avail app id";
pub(super) const MSG_AVAIL_GAS_RELAY_API_URL_PROMPT: &str = "Gas relay API URL";
pub(super) const MSG_AVAIL_GAS_RELAY_MAX_RETRIES_PROMPT: &str = "Gas relay max retries";
pub(super) const MSG_AVAIL_BRIDGE_API_URL_PROMPT: &str = "Attestation bridge API URL";
pub(super) const MSG_AVAIL_SEED_PHRASE_PROMPT: &str = "Seed phrase";
pub(super) const MSG_AVAIL_GAS_RELAY_API_KEY_PROMPT: &str = "Gas relay API key";
pub(super) const MSG_INVALID_URL_ERR: &str = "Invalid URL format";
