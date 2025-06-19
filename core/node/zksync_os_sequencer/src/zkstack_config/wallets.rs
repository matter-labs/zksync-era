#![allow(dead_code)]
//! Modified version of `zkstack_cli_config::wallets`

use alloy::signers::local::PrivateKeySigner;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use zksync_types::Address;

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct Wallet {
    pub address: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub private_key: PrivateKeySigner,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WalletsConfig {
    pub deployer: Option<Wallet>,
    pub operator: Wallet,
    pub blob_operator: Wallet,
    pub fee_account: Wallet,
    pub governor: Wallet,
    pub token_multiplier_setter: Option<Wallet>,
}
