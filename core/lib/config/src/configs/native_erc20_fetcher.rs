use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NativeErc20FetcherConfig {
    pub poll_interval: u64,
    pub host: String,
}
