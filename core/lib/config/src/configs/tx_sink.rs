use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TxSinkConfig {
    #[serde(default)]
    pub deployment_allowlist_sink: bool,
}
