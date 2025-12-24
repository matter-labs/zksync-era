use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use xshell::Shell;

use super::chain::ChainUpgradeParams;

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum SupportedPair {
    Validium,
    RollupBlobs,
    RollupCalldata,
}

#[derive(Parser, Debug, Clone)]
pub struct V30ChainUpgradeArgs {
    #[clap(flatten)]
    pub base: ChainUpgradeParams,
    #[clap(long)]
    pub da_mode: SupportedPair,
}

impl V30ChainUpgradeArgs {
    pub async fn fill_if_empty(mut self, shell: &Shell) -> anyhow::Result<Self> {
        self.base = self.base.fill_if_empty(shell).await?;
        Ok(self)
    }
}

use super::chain::UpgradeArgsInner;

impl From<V30ChainUpgradeArgs> for UpgradeArgsInner {
    fn from(value: V30ChainUpgradeArgs) -> Self {
        UpgradeArgsInner::from(value.base)
    }
}
