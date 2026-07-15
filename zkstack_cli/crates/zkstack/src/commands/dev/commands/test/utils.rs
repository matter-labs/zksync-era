use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, spinner::Spinner, wallets::Wallet};
use zkstack_cli_config::{ChainConfig, EcosystemConfig};

use crate::commands::dev::messages::{
    MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS, MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES,
};

pub const TEST_WALLETS_PATH: &str = "etc/test_config/constant/eth.json";
const AMOUNT_FOR_DISTRIBUTION_TO_TEST_WALLETS: u128 = 10_000u128 * 1_000_000_000_000_000_000u128; // 10k ETH
pub const TS_INTEGRATION_PATH: &str = "core/tests/ts-integration";

#[derive(Deserialize)]
pub struct TestWallets {
    base_path: String,
    #[serde(flatten)]
    wallets: HashMap<String, String>,
}

impl TestWallets {
    fn get(&self, id: u32) -> anyhow::Result<Wallet> {
        let mnemonic = self.wallets.get("test_mnemonic").unwrap().as_str();

        Wallet::from_mnemonic(mnemonic, &self.base_path, id)
    }

    pub fn get_main_wallet(&self) -> anyhow::Result<Wallet> {
        self.get(0)
    }

    pub fn get_test_wallet(&self, chain_config: &ChainConfig) -> anyhow::Result<Wallet> {
        self.get(chain_config.id + 100)
    }

    pub async fn init_test_wallet(
        &self,
        ecosystem_config: &EcosystemConfig,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        let wallet = self.get_test_wallet(chain_config)?;

        let l1_rpc = chain_config.get_secrets_config().await?.l1_rpc_url()?;

        zkstack_cli_common::ethereum::distribute_eth(
            self.get_main_wallet()?,
            vec![wallet.address],
            l1_rpc,
            ecosystem_config.l1_network.chain_id(),
            AMOUNT_FOR_DISTRIBUTION_TO_TEST_WALLETS,
        )
        .await?;

        Ok(())
    }
}

pub fn build_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    shell.change_dir(link_to_code.join(TS_INTEGRATION_PATH));
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS);

    Cmd::new(cmd!(shell, "yarn build")).run()?;
    Cmd::new(cmd!(shell, "yarn build-yul")).run()?;
    Cmd::new(cmd!(shell, "yarn build-evm")).run()?;

    spinner.finish();
    Ok(())
}

pub fn install_and_build_dependencies(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES);

    Cmd::new(cmd!(shell, "yarn install")).run()?;
    Cmd::new(cmd!(shell, "yarn utils build")).run()?;

    spinner.finish();
    Ok(())
}
