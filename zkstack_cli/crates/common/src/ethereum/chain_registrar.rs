use std::sync::Arc;

use ethers::{
    abi::Address,
    contract::abigen,
    prelude::{Http, Provider},
};

abigen!(
    ChainRegistar,
    r"[
        struct RegisteredChainConfig {address pendingChainAdmin;address chainAdmin;address diamondProxy;address l2BridgeAddress;}
        function getRegisteredChainConfig(uint256 chainId) public view returns (RegisteredChainConfig memory)
        event NewChainDeployed(uint256 indexed chainId, address author, address diamondProxy, address chainAdmin)
        event NewChainRegistrationProposal(uint256 indexed chainId, address author, bytes32 key)
        event SharedBridgeRegistered(uint256 indexed chainId, address l2Address)
    ]"
);

#[derive(Clone, Copy)]
pub struct ChainRegistrationResult {
    pub diamond_proxy: Address,
    pub chain_admin: Address,
    pub l2_shared_bridge: Address,
}

pub async fn load_contracts_for_chain(
    chain_registrar: Address,
    l1_rpc: String,
    l2_chain_id: u64,
) -> anyhow::Result<ChainRegistrationResult> {
    let client = Arc::new(Provider::<Http>::try_from(l1_rpc)?);
    let contract = ChainRegistar::new(chain_registrar, client);
    let (pending_chain_admin, _chain_admin, diamond_proxy, l2_bridge_address) = contract
        .get_registered_chain_config(l2_chain_id.into())
        .await?;

    Ok(ChainRegistrationResult {
        diamond_proxy,
        chain_admin: pending_chain_admin,
        l2_shared_bridge: l2_bridge_address,
    })
}
