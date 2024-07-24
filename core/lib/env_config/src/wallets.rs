use std::str::FromStr;

use anyhow::Context;
use zksync_basic_types::{Address, H256};
use zksync_config::configs::wallets::{
    AddressWallet, BaseTokenAdjuster, EthSender, StateKeeper, Wallet, Wallets,
};

use crate::FromEnv;

fn pk_from_env(env_var: &str, context: &str) -> anyhow::Result<Option<H256>> {
    std::env::var(env_var)
        .ok()
        .map(|pk| pk.parse::<H256>().context(context.to_string()))
        .transpose()
}

impl FromEnv for Wallets {
    fn from_env() -> anyhow::Result<Self> {
        let operator = pk_from_env(
            "ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY",
            "Malformed operator pk",
        )?;
        let blob_operator = pk_from_env(
            "ETH_SENDER_SENDER_OPERATOR_BLOBS_PRIVATE_KEY",
            "Malformed blob operator pk",
        )?;

        let eth_sender = if let Some(operator) = operator {
            let operator = Wallet::from_private_key_bytes(operator, None)?;
            let blob_operator = if let Some(blob_operator) = blob_operator {
                Some(Wallet::from_private_key_bytes(blob_operator, None)?)
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

        let base_token_adjuster_pk = pk_from_env(
            "BASE_TOKEN_ADJUSTER_PRIVATE_KEY",
            "Malformed base token adjuster pk",
        )?;
        let base_token_adjuster = if let Some(base_toke_adjuster_pk) = base_token_adjuster_pk {
            let wallet = Wallet::from_private_key_bytes(base_toke_adjuster_pk, None)?;
            Some(BaseTokenAdjuster { wallet })
        } else {
            None
        };

        Ok(Self {
            eth_sender,
            state_keeper,
            base_token_adjuster,
        })
    }
}
