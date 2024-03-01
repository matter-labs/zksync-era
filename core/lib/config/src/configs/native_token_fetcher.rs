use serde::Deserialize;
use zksync_basic_types::Address;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NativeTokenFetcherConfig {
    pub poll_interval: u64,
    pub host: String,
    pub token_address: Address,
}
