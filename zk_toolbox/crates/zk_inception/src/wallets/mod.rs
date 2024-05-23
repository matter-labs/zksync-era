mod config;
mod create;

pub use common::wallets::Wallet;
pub use config::WalletCreation;
pub use create::{create_localhost_wallets, create_wallets};
