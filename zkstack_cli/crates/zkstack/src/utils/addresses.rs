use ethers::prelude::Provider;
use zksync_basic_types::{
    address_to_u256,
    ethabi::{encode, Token},
    u256_to_address, Address, H256, U256,
};
use zksync_system_constants::{L2_BRIDGEHUB_ADDRESS, L2_CHAIN_ASSET_HANDLER_ADDRESS};

use crate::{abi::ChainTypeManagerAbi, utils::protocol_version::get_minor_protocol_version};

pub(crate) fn apply_l1_to_l2_alias(addr: Address) -> Address {
    let offset: Address = "1111000000000000000000000000000000001111".parse().unwrap();
    let addr_with_offset = address_to_u256(&addr) + address_to_u256(&offset);

    u256_to_address(&addr_with_offset)
}

// The most reliable way to precompute the address is to simulate `createNewChain` function
pub(crate) async fn precompute_chain_address_on_gateway(
    l2_chain_id: u64,
    base_token_asset_id: H256,
    new_l2_admin: Address,
    protocol_version: U256,
    gateway_diamond_cut: Vec<u8>,
    gw_ctm: ChainTypeManagerAbi<Provider<ethers::providers::Http>>,
) -> anyhow::Result<Address> {
    let ctm_data = encode(&[
        Token::FixedBytes(base_token_asset_id.0.into()),
        Token::Address(new_l2_admin),
        Token::Uint(protocol_version),
        Token::Bytes(gateway_diamond_cut),
    ]);

    let caller = if get_minor_protocol_version(protocol_version)?.is_pre_interop_fast_blocks() {
        L2_BRIDGEHUB_ADDRESS
    } else {
        L2_CHAIN_ASSET_HANDLER_ADDRESS
    };
    let result = gw_ctm
        .forwarded_bridge_mint(l2_chain_id.into(), ctm_data.into())
        .from(caller)
        .await?;

    Ok(result)
}
