use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TxSinkConfig {
    #[serde(default)]
    pub deployment_allowlist_sink: bool,
}

impl Default for TxSinkConfig {
    fn default() -> Self {
        Self {
            deployment_allowlist_sink: false,
        }
    }
}
