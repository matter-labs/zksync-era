use std::str::FromStr;

use anyhow::Context;
use zksync_basic_types::{Address, H256};
use zksync_config::configs::wallets::{
    AddressWallet, EthSender, StateKeeper, TokenMultiplierSetter, Wallet, Wallets,
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
        let gateway = pk_from_env(
            "ETH_SENDER_SENDER_OPERATOR_GATEWAY_PRIVATE_KEY",
            "Malformed gateway operator pk",
        )?;

        let eth_sender = if let Some(operator) = operator {
            let operator = Wallet::from_private_key_bytes(operator, None)?;
            let blob_operator = if let Some(blob_operator) = blob_operator {
                Some(Wallet::from_private_key_bytes(blob_operator, None)?)
            } else {
                None
            };
            let gateway = if let Some(gateway) = gateway {
                Some(Wallet::from_private_key_bytes(gateway, None)?)
            } else {
                None
            };

            Some(EthSender {
                operator,
                blob_operator,
                gateway,
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

        let token_multiplier_setter_pk = pk_from_env(
            "TOKEN_MULTIPLIER_SETTER_PRIVATE_KEY",
            "Malformed token multiplier setter pk",
        )?;
        let token_multiplier_setter =
            if let Some(token_multiplier_setter_pk) = token_multiplier_setter_pk {
                let wallet = Wallet::from_private_key_bytes(token_multiplier_setter_pk, None)?;
                Some(TokenMultiplierSetter { wallet })
            } else {
                None
            };

        Ok(Self {
            eth_sender,
            state_keeper,
            token_multiplier_setter,
        })
    }
}
