use crate::DAClientConfig;
use serde::Deserialize;
use zksync_basic_types::Address;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TransitionalDAClientConfig {
    /// The configuration of the data availability client that the migration is performed `to`.
    pub client_config: DAClientConfig,
    pub new_l1_da_validator_addr: Address,
    pub new_l2_da_validator_addr: Address,
}
