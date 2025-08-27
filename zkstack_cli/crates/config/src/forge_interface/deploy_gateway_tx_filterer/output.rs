use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfigTrait;

/// Represents the output config written after deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayTxFiltererOutput {
    pub gateway_tx_filterer_proxy: Address,
}

impl ZkStackConfigTrait for GatewayTxFiltererOutput {}
