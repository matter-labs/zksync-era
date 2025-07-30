use ethers::utils::hex;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{traits::ReadConfig, EcosystemConfig};
use zksync_basic_types::U256;

use crate::{
    admin_functions::{enable_validator, enable_validator_via_gateway, AdminScriptMode},
    commands::{
        chain::{
            admin_call_builder::{AdminCall, AdminCallBuilder},
            utils::{get_default_foundry_path, send_tx},
        },
        dev::commands::upgrades::{
            args::{chain::UpgradeArgsInner, v29_chain::V29ChainUpgradeArgs},
            default_chain_upgrade::{
                check_chain_readiness, fetch_chain_info, run_chain_upgrade,
                AdditionalUpgradeParams, UpdatedValidators, UpgradeInfo,
            },
            types::UpgradeVersion,
            utils::{print_error, set_upgrade_timestamp_calldata},
        },
    },
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
        },
        run_upgrade,
        crate::commands::dev::commands::upgrades::types::UpgradeVersion::V29InteropAFf,
    )
    .await
}
