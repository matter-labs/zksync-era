use clap::ValueEnum;
use serde::Deserialize;
use strum::EnumIter;
use zksync_web3_decl::jsonrpsee::core::Serialize;

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum UpgradeVersion {
    V29InteropAFf,
    V28_1Vk,
    V28_1VkEra,
}

impl UpgradeVersion {
    pub const fn get_default_upgrade_description_path(&self) -> &'static str {
        match self {
            UpgradeVersion::V29InteropAFf => {
                "./contracts/l1-contracts/script-out/v29-upgrade-ecosystem.toml"
            }
            UpgradeVersion::V28_1Vk => {
                "./contracts/l1-contracts/script-out/zk-os-v28-1-upgrade-ecosystem.toml"
            }
            UpgradeVersion::V28_1VkEra => {
                unimplemented!("V28_1VkEra is not implemented yet")
            }
        }
    }
}
