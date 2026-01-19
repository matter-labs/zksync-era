use clap::ValueEnum;
use serde::Deserialize;
use strum::EnumIter;
use zksync_web3_decl::jsonrpsee::core::Serialize;

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum UpgradeVersion {
    V29InteropAFf,
    V31InteropB,
}

impl UpgradeVersion {
    pub const fn get_default_upgrade_description_path(&self) -> &'static str {
        match self {
            UpgradeVersion::V29InteropAFf => "./l1-contracts/script-out/v29-upgrade-ecosystem.toml",
            UpgradeVersion::V31InteropB => "./l1-contracts/script-out/v31-upgrade-core.toml",
        }
    }
}
