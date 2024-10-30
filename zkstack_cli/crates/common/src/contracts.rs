use std::path::PathBuf;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn build_test_contracts(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("etc/contracts-test-data"));
    Cmd::new(cmd!(shell, "yarn install")).run()?;
    Ok(Cmd::new(cmd!(shell, "yarn build")).run()?)
}

pub fn build_l1_contracts(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l1-contracts"));
    Ok(Cmd::new(cmd!(shell, "forge build")).run()?)
}

pub fn build_l2_contracts(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l2-contracts"));
    Ok(Cmd::new(cmd!(
        shell,
        "forge build --zksync --zk-enable-eravm-extensions"
    ))
    .run()?)
}

pub struct Verifier {
    pub link_to_code: PathBuf,
    pub rpc_url: url::Url,
    pub verifier_url: url::Url,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContractSpec {
    pub name: String,
    pub address: ethers::types::Address,
    pub constructor_args: ethers::types::Bytes,
}

impl Verifier {
    pub async fn verify_l2_contract(
        &self,
        shell: &Shell,
        spec: &ContractSpec,
    ) -> anyhow::Result<()> {
        let _dir_guard = shell.push_dir(self.link_to_code.join("contracts/l2-contracts"));
        let rpc_url = self.rpc_url.to_string();
        let verifier_url = self.verifier_url.to_string();
        let address = format!("{:#x}", &spec.address);
        let name = spec.name.clone();
        let constructor_args = spec.constructor_args.to_string();
        Ok(Cmd::new(cmd!(shell, "forge verify-contract --zksync --verifier z-ksync --rpc-url {rpc_url} --verifier-url {verifier_url} {address} {name} --constructor-args {constructor_args}")).run()?)
    }
}

pub fn build_system_contracts(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/system-contracts"));
    // Do not update era-contract's lockfile to avoid dirty submodule
    Cmd::new(cmd!(shell, "yarn install --frozen-lockfile")).run()?;
    Cmd::new(cmd!(shell, "yarn preprocess:system-contracts")).run()?;
    Cmd::new(cmd!(
        shell,
        "forge build --zksync --zk-enable-eravm-extensions"
    ))
    .run()?;
    Cmd::new(cmd!(shell, "yarn preprocess:bootloader")).run()?;
    Ok(Cmd::new(cmd!(
        shell,
        "forge build --zksync --zk-enable-eravm-extensions"
    ))
    .run()?)
}
