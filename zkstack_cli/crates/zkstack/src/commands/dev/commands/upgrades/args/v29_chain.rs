use clap::Parser;
use xshell::Shell;
use zkstack_cli_config::EcosystemConfig;
use zksync_types::Address;

use super::chain::ChainUpgradeParams;

#[derive(Parser, Debug, Clone)]
pub struct V29ChainUpgradeArgs {
    #[clap(flatten)]
    pub base: ChainUpgradeParams,
    #[clap(long)]
    pub operator: Option<Address>,
    #[clap(long)]
    pub blob_operator: Option<Address>,
}

impl V29ChainUpgradeArgs {
    pub async fn fill_if_empty(mut self, shell: &Shell) -> anyhow::Result<Self> {
        self.base = self.base.fill_if_empty(shell).await?;
        // Restore operator/blob_operator default filling
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config.load_current_chain()?;
        self.operator = self
            .operator
            .or_else(|| Some(chain_config.get_wallets_config().ok()?.operator.address));
        self.blob_operator = self.blob_operator.or_else(|| {
            Some(
                chain_config
                    .get_wallets_config()
                    .ok()?
                    .blob_operator
                    .address,
            )
        });
        Ok(self)
    }
}

use super::chain::UpgradeArgsInner;

impl From<V29ChainUpgradeArgs> for UpgradeArgsInner {
    fn from(value: V29ChainUpgradeArgs) -> Self {
        UpgradeArgsInner::from(value.base)
    }
}
