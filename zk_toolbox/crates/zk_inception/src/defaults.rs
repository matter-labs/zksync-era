use config::ChainConfig;
use lazy_static::lazy_static;
use url::Url;

lazy_static! {
    pub static ref DATABASE_SERVER_URL: Url =
        Url::parse("postgres://postgres:notsecurepassword@localhost:5432").unwrap();
    pub static ref DATABASE_PROVER_URL: Url =
        Url::parse("postgres://postgres:notsecurepassword@localhost:5432").unwrap();
}

pub const ROCKS_DB_STATE_KEEPER: &str = "state_keeper";
pub const ROCKS_DB_TREE: &str = "tree";
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

pub fn generate_external_node_db_name(config: &ChainConfig) -> String {
    format!(
        "external_node_{}_{}",
        config.l1_network.to_string().to_ascii_lowercase(),
        config.name
    )
}
