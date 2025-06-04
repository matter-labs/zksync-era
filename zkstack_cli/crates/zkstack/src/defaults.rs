use lazy_static::lazy_static;
use url::Url;
use zkstack_cli_config::ChainConfig;

lazy_static! {
    pub static ref DATABASE_SERVER_URL: Url =
        Url::parse("postgres://postgres:notsecurepassword@localhost:5432").unwrap();
    pub static ref DATABASE_PROVER_URL: Url =
        Url::parse("postgres://postgres:notsecurepassword@localhost:5432").unwrap();
    pub static ref DATABASE_EXPLORER_URL: Url =
        Url::parse("postgres://postgres:notsecurepassword@localhost:5432").unwrap();
    pub static ref DATABASE_PRIVATE_RPC_URL: Url =
        Url::parse("postgres://postgres:notsecurepassword@localhost:5432").unwrap();
    pub static ref AVAIL_RPC_URL: Url = Url::parse("wss://turing-rpc.avail.so/ws").unwrap();
    pub static ref AVAIL_BRIDGE_API_URL: Url =
        Url::parse("https://turing-bridge-api.avail.so").unwrap();
}

pub const DEFAULT_OBSERVABILITY_PORT: u16 = 3000;

// Default port range
pub const PORT_RANGE_START: u16 = 3000;
pub const PORT_RANGE_END: u16 = 5000;

pub const ROCKS_DB_STATE_KEEPER: &str = "state_keeper";
pub const ROCKS_DB_TREE: &str = "tree";
pub const ROCKS_DB_PROTECTIVE_READS: &str = "protective_reads";
pub const ROCKS_DB_BASIC_WITNESS_INPUT_PRODUCER: &str = "basic_witness_input_producer";
pub const EN_ROCKS_DB_PREFIX: &str = "en";
pub const MAIN_ROCKS_DB_PREFIX: &str = "main";

pub const L2_CHAIN_ID: u32 = 271;
/// Path to base chain configuration inside zksync-era
/// Local RPC url
pub(super) const LOCAL_RPC_URL: &str = "http://127.0.0.1:8545";

pub struct DBNames {
    pub server_name: String,
    pub prover_name: String,
}

pub fn generate_db_names(config: &ChainConfig) -> DBNames {
    DBNames {
        server_name: format!(
            "zksync_server_{}_{}",
            config.l1_network.to_string().to_ascii_lowercase(),
            config.name
        ),
        prover_name: format!(
            "zksync_prover_{}_{}",
            config.l1_network.to_string().to_ascii_lowercase(),
            config.name
        ),
    }
}

pub fn generate_private_rpc_db_name(config: &ChainConfig) -> String {
    format!(
        "zksync_private_rpc_{}_{}",
        config.l1_network.to_string().to_ascii_lowercase(),
        config.name
    )
}

pub fn generate_explorer_db_name(config: &ChainConfig) -> String {
    format!(
        "zksync_explorer_{}_{}",
        config.l1_network.to_string().to_ascii_lowercase(),
        config.name
    )
}

pub fn generate_external_node_db_name(config: &ChainConfig) -> String {
    format!(
        "external_node_{}_{}",
        config.l1_network.to_string().to_ascii_lowercase(),
        config.name
    )
}
