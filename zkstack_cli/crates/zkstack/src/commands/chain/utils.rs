use std::path::PathBuf;

use ethers::{abi::encode, utils::hex};
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::ZkStackConfig;
use zksync_types::{web3::keccak256, Address, H256, L2_NATIVE_TOKEN_VAULT_ADDRESS, U256};

use crate::{
    admin_functions::AdminScriptOutput, commands::chain::admin_call_builder::AdminCallBuilder,
};

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = encode(&[
        ethers::abi::Token::Uint(l1_chain_id),
        ethers::abi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethers::abi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}

pub fn get_default_foundry_path(shell: &Shell) -> anyhow::Result<PathBuf> {
    Ok(ZkStackConfig::ecosystem(shell)?.path_to_l1_foundry())
}

pub fn display_admin_script_output(result: AdminScriptOutput) {
    let builder = AdminCallBuilder::new(result.calls);
    logger::info(format!(
        "Breakdown of calls to be performed by the chain admin:\n{}",
        builder.to_json_string()
    ));

    logger::info("\nThe calldata to be sent by the admin owner:".to_string());
    logger::info(format!("Admin address (to): {:#?}", result.admin_address));

    let (data, value) = builder.compile_full_calldata();

    logger::info(format!("Total data: {}", hex::encode(&data)));
    logger::info(format!("Total value: {}", value));
}
