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
