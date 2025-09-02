use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{git, logger, spinner::Spinner};
use zkstack_cli_config::{traits::SaveConfigWithBasePath, ZkStackConfig, ZkStackConfigTrait};

use super::{
    args::build_transactions::BuildTransactionsArgs,
    common::deploy_l1,
    create_configs::create_initial_deployments_config,
    utils::{build_system_contracts, install_yarn_dependencies},
};
use crate::messages::{
    MSG_BUILDING_ECOSYSTEM, MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_ECOSYSTEM_TXN_OUTRO,
    MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR, MSG_INTALLING_DEPS_SPINNER,
    MSG_WRITING_OUTPUT_FILES_SPINNER,
};

const DEPLOY_TRANSACTIONS_FILE_SRC: &str =
    "contracts/l1-contracts/broadcast/DeployL1.s.sol/9/dry-run/run-latest.json";
const DEPLOY_TRANSACTIONS_FILE_DST: &str = "deploy-l1-txns.json";

const SCRIPT_CONFIG_FILE_SRC: &str = "contracts/l1-contracts/script-config/config-deploy-l1.toml";
const SCRIPT_CONFIG_FILE_DST: &str = "config-deploy-l1.toml";

pub async fn run(args: BuildTransactionsArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    git::submodule_update(shell, &ecosystem_config.link_to_code())?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    logger::info(MSG_BUILDING_ECOSYSTEM);

    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code())?;
    build_system_contracts(shell, &ecosystem_config.link_to_code())?;
    spinner.finish();

    let spinner = Spinner::new(MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1(
        shell,
        &args.forge_args,
        &ecosystem_config,
        &initial_deployment_config,
        &args.l1_rpc_url,
        Some(args.sender),
        false,
        false,
        None,
    )
    .await?;

    contracts_config.save_with_base_path(shell, &args.out)?;
    spinner.finish();

    let spinner = Spinner::new(MSG_WRITING_OUTPUT_FILES_SPINNER);
    shell
        .create_dir(&args.out)
        .context(MSG_ECOSYSTEM_TXN_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        ecosystem_config
            .link_to_code()
            .join(DEPLOY_TRANSACTIONS_FILE_SRC),
        args.out.join(DEPLOY_TRANSACTIONS_FILE_DST),
    )?;

    shell.copy_file(
        ecosystem_config.link_to_code().join(SCRIPT_CONFIG_FILE_SRC),
        args.out.join(SCRIPT_CONFIG_FILE_DST),
    )?;
    spinner.finish();

    logger::outro(MSG_ECOSYSTEM_TXN_OUTRO);

    Ok(())
}
