use serde::{Deserialize, Serialize};
use zksync_basic_types::secrets::{APIKey, SeedPhrase};

pub const AVAIL_GAS_RELAY_CLIENT_NAME: &str = "GasRelay";
pub const AVAIL_FULL_CLIENT_NAME: &str = "FullClient";

pub const IN_BLOCK_FINALITY_STATE: &str = "inBlock";
pub const FINALIZED_FINALITY_STATE: &str = "finalized";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "avail_client")]
pub enum AvailClientConfig {
    FullClient(AvailDefaultConfig),
    GasRelay(AvailGasRelayConfig),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailConfig {
    pub bridge_api_url: String,
    pub timeout_ms: usize,
    #[serde(flatten)]
    pub config: AvailClientConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailDefaultConfig {
    pub api_node_url: String,
    pub app_id: u32,
    pub finality_state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailGasRelayConfig {
    pub gas_relay_api_url: String,
    pub max_retries: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
    pub gas_relay_api_key: Option<APIKey>,
}

impl AvailDefaultConfig {
    pub fn finality_state(&self) -> anyhow::Result<String> {
        match self.finality_state.clone() {
            Some(finality_state) => match finality_state.as_str() {
                IN_BLOCK_FINALITY_STATE | FINALIZED_FINALITY_STATE => Ok(finality_state),
                _ => Err(anyhow::anyhow!(
                    "Invalid finality state: {}. Supported values are: {}, {}",
                    finality_state,
                    IN_BLOCK_FINALITY_STATE,
                    FINALIZED_FINALITY_STATE
                )),
            },
            None => Ok(IN_BLOCK_FINALITY_STATE.to_string()),
        }
    }
}
