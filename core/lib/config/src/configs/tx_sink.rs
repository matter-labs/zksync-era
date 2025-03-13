use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TxSinkConfig {
    pub use_whitelisted_sink: Option<bool>,
}
