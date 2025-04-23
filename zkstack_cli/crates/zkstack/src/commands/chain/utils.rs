use std::{num::NonZeroUsize, path::PathBuf, str::FromStr, sync::Arc};

use anyhow::Context;
use ethers::{
    abi::encode,
    providers::{Http, Provider},
    utils::hex,
};
use xshell::Shell;
use zkstack_cli_config::EcosystemConfig;
use zksync_types::{
    url::SensitiveUrl, web3::keccak256, Address, L2ChainId, H256, L2_NATIVE_TOKEN_VAULT_ADDRESS,
    U256,
};
use zksync_web3_decl::client::{Client, L2};

use crate::{
    accept_ownership::AdminScriptOutput, commands::chain::admin_call_builder::AdminCallBuilder,
};

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = encode(&[
        ethers::abi::Token::Uint(l1_chain_id),
        ethers::abi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethers::abi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}

pub fn get_ethers_provider(url: &str) -> anyhow::Result<Arc<Provider<Http>>> {
    let provider = match Provider::<Http>::try_from(url) {
        Ok(provider) => provider,
        Err(err) => {
            anyhow::bail!("Connection error: {:#?}", err);
        }
    };

    Ok(Arc::new(provider))
}

pub fn get_zk_client(url: &str, l2_chain_id: u64) -> anyhow::Result<Client<L2>> {
    let client = Client::http(SensitiveUrl::from_str(url).unwrap())
        .context("failed creating JSON-RPC client for main node")?
        .for_network(L2ChainId::new(l2_chain_id).unwrap().into())
        .with_allowed_requests_per_second(NonZeroUsize::new(100_usize).unwrap())
        .build();

    Ok(client)
}

pub fn get_default_foundry_path(shell: &Shell) -> anyhow::Result<PathBuf> {
    Ok(EcosystemConfig::from_file(shell)?.path_to_l1_foundry())
}

pub fn display_admin_script_output(result: AdminScriptOutput) {
    let builder = AdminCallBuilder::new(result.calls);
    logger::info(format!(
        "Breakdown of calls to be performed by the chain admin:\n{}",
        builder.to_json_string()
    ));

    logger::info(format!("\nThe calldata to be sent by the admin owner:"));
    logger::info(format!("Admin address (to): {:#?}", result.admin_address));

    let (data, value) = builder.compile_full_calldata();

    logger::info(format!("Total data: {}", hex::encode(&data)));
    logger::info(format!("Total value: {}", value));
}
