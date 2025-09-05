use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, L2ChainId, H256};

use crate::traits::FileConfigTrait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupLegacyBridgeInput {
    pub bridgehub: Address,
    pub diamond_proxy: Address,
    pub shared_bridge_proxy: Address,
    pub l1_nullifier_proxy: Address,
    pub l1_native_token_vault: Address,
    pub transparent_proxy_admin: Address,
    pub erc20bridge_proxy: Address,
    pub token_weth_address: Address,
    pub chain_id: L2ChainId,
    pub l2shared_bridge_address: Address,
    pub create2factory_salt: H256,
    pub create2factory_addr: Address,
}

impl FileConfigTrait for SetupLegacyBridgeInput {}
