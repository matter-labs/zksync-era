use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, H256};

use crate::traits::ZkStackConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPreparationOutput {
    pub governance_l2_tx_hash: H256,
    pub l2_chain_admin_address: Address,
    pub gateway_transaction_filterer_implementation: Address,
    pub gateway_transaction_filterer_proxy: Address,
}

impl ZkStackConfig for GatewayPreparationOutput {}
