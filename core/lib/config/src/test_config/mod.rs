// Built-in deps
use std::fs;
// External uses
use serde::Deserialize;
// Workspace uses
// Local uses

/// Transforms relative path like `constant/some_file.json` into full path like
/// `$ZKSYNC_HOME/etc/test_config/constant/some_file.json`.
fn config_path(postfix: &str) -> String {
    let home = std::env::var("ZKSYNC_HOME").expect("ZKSYNC_HOME variable must be set");

    format!("{}/etc/test_config/{}", home, postfix)
}

fn load_json(path: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("Invalid config path"))
        .expect("Invalid config format")
}

/// Common Ethereum parameters.
#[derive(Debug, Deserialize)]
pub struct EthConfig {
    /// Set of 12 words for connecting to an Ethereum wallet.
    pub test_mnemonic: String,
}

/// Common Api addresses.
#[derive(Debug, Deserialize)]
pub struct ApiConfig {
    /// Address of the rest api.
    pub rest_api_url: String,
}

macro_rules! impl_config {
    ($name_config:ident, $file:tt) => {
        impl $name_config {
            pub fn load() -> Self {
                let object = load_json(&config_path(&format!("{}.json", $file)));
                serde_json::from_value(object)
                    .expect(&format!("Cannot deserialize config from '{}'", $file))
            }
        }
    };
}

impl_config!(ApiConfig, "constant/api");
impl_config!(EthConfig, "constant/eth");

#[derive(Debug)]
pub struct TestConfig {
    pub eth: EthConfig,
    pub api: ApiConfig,
}

impl TestConfig {
    pub fn load() -> Self {
        Self {
            eth: EthConfig::load(),
            api: ApiConfig::load(),
        }
    }
}
