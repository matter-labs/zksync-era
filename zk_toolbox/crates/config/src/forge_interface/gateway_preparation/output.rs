use serde::{Deserialize, Serialize};
use zksync_basic_types::H256;

use crate::traits::ZkToolboxConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPreparationOutput {
    pub governance_l2_tx_hash: H256,
}

impl ZkToolboxConfig for GatewayPreparationOutput {}
