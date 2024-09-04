use anyhow::Context;
use common::{forge::Forge, git, logger, spinner::Spinner};
use config::{
    forge_interface::{
        deploy_ecosystem::input::DeployL1Config, script_params::DEPLOY_ECOSYSTEM_SCRIPT_PARAMS,
    },
    traits::{ReadConfigWithBasePath, SaveConfig},
    EcosystemConfig, GenesisConfig,
};
use types::ProverMode;
use xshell::Shell;

use super::{
    args::build::EcosystemBuildArgs,
    create_configs::create_initial_deployments_config,
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
    let final_ecosystem_args = args.fill_values_with_prompt();

    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
    build_system_contracts(shell, &ecosystem_config.link_to_code)?;
    spinner.finish();

    let default_genesis_config =
        GenesisConfig::read_with_base_path(shell, ecosystem_config.get_default_configs_path())
            .context("Context")?;

    let wallets_config = ecosystem_config.get_wallets()?;
    // For deploying ecosystem we only need genesis batch params
    let deploy_config = DeployL1Config::new(
        &default_genesis_config,
        &wallets_config,
        &initial_deployment_config,
        ecosystem_config.era_chain_id,
        ecosystem_config.prover_version == ProverMode::NoProofs,
    );
    let deploy_config_path = DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.input(&ecosystem_config.link_to_code);
    deploy_config.save(shell, deploy_config_path)?;

    let spinner = Spinner::new(MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER);
    let forge = Forge::new(&ecosystem_config.path_to_foundry())
        .script(
            &DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(),
            final_ecosystem_args.forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(final_ecosystem_args.l1_rpc_url)
        .with_sender(final_ecosystem_args.sender);

    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    shell
        .create_dir(&final_ecosystem_args.out)
        .context(MSG_ECOSYSTEM_BUILD_OUT_PATH_INVALID_ERR)?;

    shell.copy_file(
        format!(
            "{}/{}",
            ecosystem_config.link_to_code.to_string_lossy(),
            DEPLOY_TRANSACTIONS_FILE
        ),
        format!("{}/deploy.json", final_ecosystem_args.out),
    )?;

    logger::outro(MSG_ECOSYSTEM_BUILD_OUTRO);

    Ok(())
}
