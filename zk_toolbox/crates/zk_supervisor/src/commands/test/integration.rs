use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use common::{cmd::Cmd, config::global_config, logger, spinner::Spinner};
use config::EcosystemConfig;
use ethers::{
    signers::{coins_bip39::English, MnemonicBuilder},
    utils::hex,
};
use serde::Deserialize;
use xshell::{cmd, Shell};

use super::{args::integration::IntegrationArgs, fund::fund_test_wallet};
use crate::messages::{
    msg_integration_tests_run, MSG_CHAIN_NOT_FOUND_ERR, MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS,
    MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES, MSG_INTEGRATION_TESTS_RUN_SUCCESS,
};

const TS_INTEGRATION_PATH: &str = "core/tests/ts-integration";
const TEST_WALLETS_PATH: &str = "etc/test_config/constant/eth.json";
const CONTRACTS_TEST_DATA_PATH: &str = "etc/contracts-test-data";

pub async fn run(shell: &Shell, args: IntegrationArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    shell.change_dir(ecosystem_config.link_to_code.join(TS_INTEGRATION_PATH));

    logger::info(msg_integration_tests_run(args.external_node));

    if !args.no_deps {
        build_repository(shell, &ecosystem_config)?;
        build_test_contracts(shell, &ecosystem_config)?;
    }

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let test_wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context("Impossible to deserialize test wallets")?;
    let test_wallet_id: String = format!("test_mnemonic{}", chain_config.id + 1);

    let phrase = test_wallets
        .wallets
        .get(test_wallet_id.as_str())
        .unwrap()
        .as_str();

    let secret_key = hex::encode(
        MnemonicBuilder::<English>::default()
            .phrase(phrase)
            .derivation_path(test_wallets.base_path.as_str())
            .context(format!(
                "Impossible to parse derivation path: {}",
                test_wallets.base_path
            ))?
            .build()
            .context(format!("Impossible to parse mnemonic: {}", phrase))?
            .signer()
            .to_bytes(),
    );

    logger::info(format!("MASTER_WALLET_PK: {}", secret_key));

    fund_test_wallet(shell, &ecosystem_config, &chain_config).await?;

    let mut command = cmd!(shell, "yarn jest --forceExit --testTimeout 60000")
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("MASTER_WALLET_PK", secret_key);

    if args.external_node {
        command = command.env("EXTERNAL_NODE", format!("{:?}", args.external_node))
    }

    if global_config().verbose {
        command = command.env(
            "ZKSYNC_DEBUG_LOGS",
            format!("{:?}", global_config().verbose),
        )
    }

    Cmd::new(command).with_force_run().run()?;

    logger::outro(MSG_INTEGRATION_TESTS_RUN_SUCCESS);

    Ok(())
}

fn build_repository(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(&ecosystem_config.link_to_code);
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES);

    Cmd::new(cmd!(shell, "yarn install --frozen-lockfile")).run()?;
    Cmd::new(cmd!(shell, "yarn utils build")).run()?;

    spinner.finish();
    Ok(())
}

fn build_test_contracts(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS);

    Cmd::new(cmd!(shell, "yarn build")).run()?;
    Cmd::new(cmd!(shell, "yarn build-yul")).run()?;

    let _dir_guard = shell.push_dir(ecosystem_config.link_to_code.join(CONTRACTS_TEST_DATA_PATH));
    Cmd::new(cmd!(shell, "yarn build")).run()?;

    spinner.finish();
    Ok(())
}

#[derive(Deserialize)]
struct TestWallets {
    #[serde(rename = "web3_url")]
    _web3_url: String,
    #[serde(rename = "mnemonic")]
    _mnemonic: String,
    base_path: String,
    #[serde(flatten)]
    wallets: HashMap<String, String>,
}
