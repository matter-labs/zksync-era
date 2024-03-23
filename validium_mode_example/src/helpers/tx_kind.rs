use std::{fmt::Display, sync::Arc};

use colored::Colorize;
use ethers::{
    abi::Address, core::k256::ecdsa::SigningKey, providers::Http, types::TransactionReceipt,
};
use zksync_web3_rs::{providers::Provider, ZKSWallet};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TxKind {
    Deploy,
    Mint,
    Transfer,
}

impl TxKind {
    pub async fn run(
        &self,
        zks_wallet: Arc<ZKSWallet<Provider<Http>, SigningKey>>,
        erc20_address: Address,
        tx_counter: usize,
        txs_per_account: usize,
    ) -> TransactionReceipt {
        println!(
            "{}",
            format!(
                "{account} {self} {tx_counter}/{txs_per_account}",
                account = hex::encode(zks_wallet.l2_address().as_bytes())
            )
            .bright_yellow()
        );
        match self {
            TxKind::Deploy => super::deploy(zks_wallet).await,
            TxKind::Mint => super::mint(zks_wallet, erc20_address).await,
            TxKind::Transfer => super::transfer(zks_wallet, erc20_address).await,
        }
    }
}

impl Display for TxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxKind::Deploy => write!(f, "Deploy"),
            TxKind::Mint => write!(f, "Mint"),
            TxKind::Transfer => write!(f, "Transfer"),
        }
    }
}
