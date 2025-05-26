use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeployTeeOutput {
    pub tee_dcap_attestation_addr: Address,
    pub hash_validator_addr: Address,
    pub p256_verifier_addr: Address,
}

impl ZkStackConfig for DeployTeeOutput {}
