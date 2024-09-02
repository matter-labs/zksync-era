use std::collections::HashMap;

use anyhow::Context;
use common::{cmd::Cmd, spinner::Spinner, wallets::Wallet};
use config::{ChainConfig, EcosystemConfig};
use ethers::{
    providers::{Http, Middleware, Provider},
    utils::hex::ToHex,
};
use serde::Deserialize;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS, MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES,
};

pub const TEST_WALLETS_PATH: &str = "etc/test_config/constant/eth.json";
const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

const TS_INTEGRATION_PATH: &str = "core/tests/ts-integration";
const CONTRACTS_TEST_DATA_PATH: &str = "etc/contracts-test-data";

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
        self.get(chain_config.id)
    }

    pub fn get_test_pk(&self, chain_config: &ChainConfig) -> anyhow::Result<String> {
        self.get_test_wallet(chain_config)?
            .private_key
            .ok_or(anyhow::Error::msg("Private key not found"))
            .map(|pk| pk.encode_hex::<String>())
    }

    pub async fn init_test_wallet(
        &self,
        ecosystem_config: &EcosystemConfig,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<()> {
        let wallet = self.get_test_wallet(chain_config)?;

        let l1_rpc = chain_config
            .get_secrets_config()?
            .l1
            .context("No L1 secrets available")?
            .l1_rpc_url
            .expose_str()
            .to_owned();

        let provider = Provider::<Http>::try_from(l1_rpc.clone())?;
        let balance = provider.get_balance(wallet.address, None).await?;

        if balance.is_zero() {
            common::ethereum::distribute_eth(
                self.get_main_wallet()?,
                vec![wallet.address],
                l1_rpc,
                ecosystem_config.l1_network.chain_id(),
                AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
            )
            .await?
        }

        Ok(())
    }
}

pub fn build_contracts(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    shell.change_dir(ecosystem_config.link_to_code.join(TS_INTEGRATION_PATH));
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS);

    Cmd::new(cmd!(shell, "yarn build")).run()?;
    Cmd::new(cmd!(shell, "yarn build-yul")).run()?;

    let _dir_guard = shell.push_dir(ecosystem_config.link_to_code.join(CONTRACTS_TEST_DATA_PATH));
    Cmd::new(cmd!(shell, "yarn build")).run()?;

    spinner.finish();
    Ok(())
}

pub fn install_and_build_dependencies(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(&ecosystem_config.link_to_code);
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES);

    Cmd::new(cmd!(shell, "yarn install")).run()?;
    Cmd::new(cmd!(shell, "yarn utils build")).run()?;

    spinner.finish();
    Ok(())
}
