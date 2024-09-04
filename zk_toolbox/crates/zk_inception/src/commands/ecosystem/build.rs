use anyhow::Context;
use common::{forge::Forge, git, logger, spinner::Spinner};
use config::{forge_interface::script_params::DEPLOY_ECOSYSTEM_SCRIPT_PARAMS, EcosystemConfig};
use xshell::Shell;

use super::{
    args::build::EcosystemBuildArgs,
    utils::{build_system_contracts, install_yarn_dependencies},
};
use crate::{
    messages::{
        MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_ECOSYSTEM_BUILD_OUTRO,
        MSG_ECOSYSTEM_BUILD_OUT_PATH_INVALID_ERR, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::check_the_balance,
};

const DEPLOY_TRANSACTIONS_FILE: &str =
    "contracts/l1-contracts/broadcast/DeployL1.s.sol/9/dry-run/run-latest.json";

pub async fn run(args: EcosystemBuildArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let sender = args.sender.clone();
    let final_ecosystem_args = args.fill_values_with_prompt();

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
    build_system_contracts(shell, &ecosystem_config.link_to_code)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER);
    let forge = Forge::new(&ecosystem_config.path_to_foundry())
        .script(
            &DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(),
            final_ecosystem_args.forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(final_ecosystem_args.ecosystem.l1_rpc_url)
        .with_sender(sender);

    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    shell
        .create_dir(&final_ecosystem_args.out)
        .context(MSG_ECOSYSTEM_BUILD_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        DEPLOY_TRANSACTIONS_FILE,
        format!("{}/deploy.json", final_ecosystem_args.out),
    )?;

    logger::outro(MSG_ECOSYSTEM_BUILD_OUTRO);

    Ok(())
}
