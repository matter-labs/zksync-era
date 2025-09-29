use std::path::Path;

use xshell::{cmd, Shell};
use zksync_system_constants::L2_NATIVE_TOKEN_VAULT_ADDRESS;
use zksync_types::{ethabi::encode, web3::keccak256, Address, H256, U256};

use crate::cmd::Cmd;

pub fn build_l1_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("l1-contracts"));
    // Do not update era-contract's lockfile to avoid dirty submodule
    // Note, tha the v26 contracts depend on the node_modules to be present at the time of the compilation.
    Cmd::new(cmd!(shell, "yarn install --frozen-lockfile")).run()?;
    Ok(Cmd::new(cmd!(shell, "yarn build:foundry")).run()?)
}

pub fn build_l1_da_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("da-contracts"));
    Ok(Cmd::new(cmd!(shell, "forge build")).run()?)
}

pub fn build_l2_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("l2-contracts"));
    Cmd::new(cmd!(shell, "yarn build:foundry")).run()?;
    Ok(())
}

pub fn build_system_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("system-contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn build:foundry")).run()?)
}

pub fn install_yarn_dependencies(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    Ok(Cmd::new(cmd!(shell, "yarn install")).run()?)
}

pub fn build_da_contracts(shell: &Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts);
    Ok(Cmd::new(cmd!(shell, "yarn da build:foundry")).run()?)
}

pub fn rebuild_all_contracts(shell: &Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    install_yarn_dependencies(shell, link_to_contracts)?;
    build_l1_contracts(shell.clone(), link_to_contracts)?;
    build_l1_da_contracts(shell.clone(), link_to_contracts)?;
    build_l2_contracts(shell.clone(), link_to_contracts)?;
    build_system_contracts(shell.clone(), link_to_contracts)?;
    build_da_contracts(shell, link_to_contracts)?;
    Ok(())
}

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = encode(&[
        ethers::abi::Token::Uint(l1_chain_id),
        ethers::abi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethers::abi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}
