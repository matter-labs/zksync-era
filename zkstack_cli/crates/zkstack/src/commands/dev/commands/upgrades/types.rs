use clap::ValueEnum;
use serde::Deserialize;
use strum::EnumIter;
use zksync_web3_decl::jsonrpsee::core::Serialize;

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum UpgradeVersions {
    V29InteropAFf,
    V28_1Vk,
}

impl UpgradeVersions {
    pub const fn get_default_upgrade_description_path(&self) -> &'static str {
        match self {
            UpgradeVersions::V29InteropAFf => "./contracts/l1-contracts/script-out/v29-upgrade-ecosystem.toml",
            UpgradeVersions::V28_1Vk => "./contracts/l1-contracts/script-out/zk-os-v28-1-upgrade-ecosystem.toml",
        }
    }
}
