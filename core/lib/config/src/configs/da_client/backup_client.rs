use crate::DAClientConfig;
use serde::Deserialize;
use zksync_basic_types::Address;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BackupDAClientConfig {
    /// The configuration of the backup DA client.
    pub client_config: DAClientConfig,
    /// The address of the L1 DA validator that will be set via `setDAValidatorPair`.
    pub new_l1_da_validator_addr: Address,
    /// The address of the L2 DA validator that will be set via `setDAValidatorPair`.
    /// This contract must be deployed before the DA transition is scheduled.
    pub new_l2_da_validator_addr: Address,
}
