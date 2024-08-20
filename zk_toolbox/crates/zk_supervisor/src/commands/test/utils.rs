use std::collections::HashMap;

use anyhow::Context;
use common::wallets::Wallet;
use config::{ChainConfig, EcosystemConfig};
use ethers::{
    providers::{Http, Middleware, Provider},
    signers::{coins_bip39::English, MnemonicBuilder},
    types::H256,
};
use serde::Deserialize;

pub const TEST_WALLETS_PATH: &str = "etc/test_config/constant/eth.json";
const AMOUNT_FOR_DISTRIBUTION_TO_WALLETS: u128 = 1000000000000000000000;

#[derive(Deserialize)]
pub struct TestWallets {
    base_path: String,
    #[serde(rename = "web3_url")]
    _web3_url: String,
    #[serde(rename = "mnemonic")]
    _mnemonic: String,
    #[serde(flatten)]
    wallets: HashMap<String, String>,
}

impl TestWallets {
    pub fn get(&self, id: String) -> anyhow::Result<Wallet> {
        let mnemonic = self.wallets.get(id.as_str()).unwrap().as_str();
        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .derivation_path(&self.base_path)?
            .build()?;
        let private_key = H256::from_slice(&wallet.signer().to_bytes());

        Ok(Wallet::new_with_key(private_key))
    }

    pub fn get_main_wallet(&self) -> anyhow::Result<Wallet> {
        self.get("test_mnemonic".to_string())
    }

    pub fn get_test_wallet(&self, chain_config: &ChainConfig) -> anyhow::Result<Wallet> {
        let test_wallet_id: String = format!("test_mnemonic{}", chain_config.id + 1);

        self.get(test_wallet_id)
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
