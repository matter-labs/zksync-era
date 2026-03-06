use xshell::Shell;

use crate::commands::dev::commands::upgrades::{
    args::v29_chain::V29ChainUpgradeArgs,
    default_chain_upgrade::run_chain_upgrade,
};

pub(crate) async fn run(
    shell: &Shell,
    args_input: V29ChainUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    run_chain_upgrade(
        shell,
        args_input.base.clone(),
        run_upgrade,
        crate::commands::dev::commands::upgrades::types::UpgradeVersion::V29InteropAFf,
    )
    .await
}
