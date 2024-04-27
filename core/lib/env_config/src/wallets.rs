use std::str::FromStr;

use anyhow::Context;
use zksync_basic_types::Address;
use zksync_config::configs::wallets::{AddressWallet, EthSender, StateKeeper, Wallet, Wallets};

use crate::FromEnv;

impl FromEnv for Wallets {
    fn from_env() -> anyhow::Result<Self> {
        let operator = std::env::var("ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY")
            .ok()
            .map(|pk| pk.parse().context("Malformed pk"))
            .transpose()?;

        let blob_operator = std::env::var("ETH_SENDER_SENDER_OPERATOR_BLOBS_PRIVATE_KEY")
            .ok()
            .map(|pk| pk.parse().context("Malformed pk"))
            .transpose()?;

        let eth_sender = if let Some(operator) = operator {
            let operator = Wallet::from_private_key(operator, None)?;
            let blob_operator = if let Some(blob_operator) = blob_operator {
                Some(Wallet::from_private_key(blob_operator, None)?)
            } else {
                None
            };
            Some(EthSender {
                operator,
                blob_operator,
            })
        } else {
            None
        };

        let fee_account = std::env::var("CHAIN_STATE_KEEPER_FEE_ACCOUNT_ADDR").ok();
        let state_keeper = if let Some(fee_account) = fee_account {
            let fee_account = AddressWallet::from_address(Address::from_str(&fee_account)?);
            Some(StateKeeper { fee_account })
        } else {
            None
        };

        Ok(Self {
            eth_sender,
            state_keeper,
        })
    }
}
