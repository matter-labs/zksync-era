use std::path::PathBuf;

use anyhow::Context;
use common::{config::global_config, spinner::Spinner};
use config::EcosystemConfig;
use ethers::{
    signers::{coins_bip39::English, MnemonicBuilder, Signer},
    types::H160,
};
use serde::Deserialize;
use types::{L1Network, WalletCreation};
use xshell::Shell;

use crate::messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_DISTRIBUTING_ETH_SPINNER};

const TEST_WALLETS_PATH: &str = "etc/test_config/constant/eth.json";
const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;

    let chain = ecosystem
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)
        .unwrap();

    if chain.wallet_creation == WalletCreation::Localhost
        && ecosystem.l1_network == L1Network::Localhost
    {
        let spinner = Spinner::new(MSG_DISTRIBUTING_ETH_SPINNER);

        let wallets_path: PathBuf = ecosystem.link_to_code.join(TEST_WALLETS_PATH);
        let test_wallets: TestWallets =
            serde_json::from_str(shell.read_file(&wallets_path)?.as_ref()).unwrap();

        let wallets = ecosystem.get_wallets()?;

        common::ethereum::distribute_eth(
            wallets.operator,
            test_wallets.address_list(),
            chain
                .get_secrets_config()?
                .l1
                .unwrap()
                .l1_rpc_url
                .expose_str()
                .to_owned(),
            ecosystem.l1_network.chain_id(),
            AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
        )
        .await?;

        spinner.finish();
    }

    Ok(())
}

#[derive(Deserialize)]
struct TestWallets {
    _web3_url: String,
    _test_mnemonic: String,
    test_mnemonic2: String,
    test_mnemonic3: String,
    test_mnemonic4: String,
    test_mnemonic5: String,
    test_mnemonic6: String,
    test_mnemonic7: String,
    test_mnemonic8: String,
    test_mnemonic9: String,
    test_mnemonic10: String,
    _mnemonic: String,
    base_path: String,
}

impl TestWallets {
    pub fn address_list(&self) -> Vec<H160> {
        let address_from_string = |s: &String| {
            MnemonicBuilder::<English>::default()
                .phrase(s.as_str())
                .derivation_path(self.base_path.as_str())
                .unwrap()
                .build()
                .unwrap()
                .address()
        };

        vec![
            address_from_string(&self.test_mnemonic2),
            address_from_string(&self.test_mnemonic3),
            address_from_string(&self.test_mnemonic4),
            address_from_string(&self.test_mnemonic5),
            address_from_string(&self.test_mnemonic6),
            address_from_string(&self.test_mnemonic7),
            address_from_string(&self.test_mnemonic8),
            address_from_string(&self.test_mnemonic9),
            address_from_string(&self.test_mnemonic10),
        ]
    }
}
