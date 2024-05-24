use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ValueEnum,
    EnumIter,
    strum_macros::Display,
)]
pub enum WalletCreation {
    /// Load wallets from localhost mnemonic, they are funded for localhost env
    #[default]
    Localhost,
    /// Generate random wallets
    Random,
    /// Generate placeholder wallets
    Empty,
    /// Specify file with wallets
    InFile,
}
