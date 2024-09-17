use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub struct EigenDAConfig {
    pub api_node_url: String,
    pub custom_quorum_numbers: Option<Vec<u32>>,
    pub account_id: Option<String>,
}
