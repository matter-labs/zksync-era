use xshell::Shell;

use crate::commands::dev::commands::upgrades::{
    args::v29_chain::V29ChainUpgradeArgs,
    default_chain_upgrade::{run_chain_upgrade, AdditionalUpgradeParams, UpdatedValidators},
};

pub(crate) async fn run(
    shell: &Shell,
    args_input: V29ChainUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    run_chain_upgrade(
        shell,
        args_input.base.clone(),
        AdditionalUpgradeParams {
            updated_validators: Some(UpdatedValidators {
                operator: args_input.operator,
                blob_operator: args_input.blob_operator,
            }),
            updated_da_validators: None,
        },
        run_upgrade,
        crate::commands::dev::commands::upgrades::types::UpgradeVersion::V29InteropAFf,
    )
    .await
}
