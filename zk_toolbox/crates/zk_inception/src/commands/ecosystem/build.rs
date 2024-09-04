use anyhow::Context;
use common::{
    files::save_toml_file,
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
};
use config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{DeployL1Config, InitialDeploymentConfig},
            output::DeployL1Output,
        },
        script_params::DEPLOY_ECOSYSTEM_SCRIPT_PARAMS,
    },
    traits::{
        FileConfigWithDefaultName, ReadConfig, ReadConfigWithBasePath, SaveConfig,
        SaveConfigWithBasePath,
    },
    ContractsConfig, EcosystemConfig, GenesisConfig,
};
use types::{L1Network, ProverMode};
use xshell::Shell;

use super::{
    args::build::{EcosystemArgsFinal, EcosystemBuildArgs, EcosystemBuildArgsFinal},
    utils::{build_system_contracts, install_yarn_dependencies},
};
use crate::{
    commands::ecosystem::create_configs::create_initial_deployments_config,
    messages::{
        msg_ecosystem_no_found_preexisting_contract, MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER,
        MSG_ECOSYSTEM_BUILD_CONTRACTS_PATH_INVALID_ERR, MSG_ECOSYSTEM_BUILD_OUTRO,
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

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let sender = args.sender.clone();
    let mut final_ecosystem_args = args.fill_values_with_prompt();

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    let contracts_config = init(
        &mut final_ecosystem_args,
        shell,
        &ecosystem_config,
        &initial_deployment_config,
        sender.clone(),
    )
    .await?;

    shell
        .create_dir(&final_ecosystem_args.out)
        .context(MSG_ECOSYSTEM_BUILD_OUT_PATH_INVALID_ERR)?;

    save_toml_file(
        shell,
        format!("{}/contracts.toml", final_ecosystem_args.out),
        contracts_config,
        "",
    )
    .context(MSG_ECOSYSTEM_BUILD_CONTRACTS_PATH_INVALID_ERR)?;

    shell.copy_file(
        DEPLOY_TRANSACTIONS_FILE,
        format!("{}/deploy.json", final_ecosystem_args.out),
    )?;

    logger::outro(MSG_ECOSYSTEM_BUILD_OUTRO);

    Ok(())
}

async fn init(
    init_args: &mut EcosystemBuildArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    sender: String,
) -> anyhow::Result<ContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
    build_system_contracts(shell, &ecosystem_config.link_to_code)?;
    spinner.finish();

    let contracts = build_ecosystem(
        shell,
        &mut init_args.ecosystem,
        init_args.forge_args.clone(),
        ecosystem_config,
        initial_deployment_config,
        sender,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(contracts)
}

async fn build_ecosystem(
    shell: &Shell,
    ecosystem: &mut EcosystemArgsFinal,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    sender: String,
) -> anyhow::Result<ContractsConfig> {
    if ecosystem.build_ecosystem {
        return build_ecosystem_inner(
            shell,
            forge_args,
            ecosystem_config,
            initial_deployment_config,
            sender,
        )
        .await;
    }

    let ecosystem_preexisting_configs_path =
        ecosystem_config
            .get_preexisting_configs_path()
            .join(format!(
                "{}.yaml",
                ecosystem_config.l1_network.to_string().to_lowercase()
            ));

    // currently there are not some preexisting ecosystem contracts in
    // chains, so we need check if this file exists.
    if ecosystem.ecosystem_contracts_path.is_none() && !ecosystem_preexisting_configs_path.exists()
    {
        anyhow::bail!(msg_ecosystem_no_found_preexisting_contract(
            &ecosystem_config.l1_network.to_string()
        ))
    }

    let ecosystem_contracts_path =
        ecosystem
            .ecosystem_contracts_path
            .clone()
            .unwrap_or_else(|| match ecosystem_config.l1_network {
                L1Network::Localhost => {
                    ContractsConfig::get_path_with_base_path(&ecosystem_config.config)
                }
                L1Network::Sepolia | L1Network::Holesky | L1Network::Mainnet => {
                    ecosystem_preexisting_configs_path
                }
            });

    ContractsConfig::read(shell, ecosystem_contracts_path)
}

async fn build_ecosystem_inner(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    sender: String,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.input(&config.link_to_code);

    let default_genesis_config =
        GenesisConfig::read_with_base_path(shell, config.get_default_configs_path())
            .context("Context")?;

    let wallets_config = config.get_wallets()?;

    // For deploying ecosystem we only need genesis batch params
    let deploy_config = DeployL1Config::new(
        &default_genesis_config,
        &wallets_config,
        initial_deployment_config,
        config.era_chain_id,
        config.prover_version == ProverMode::NoProofs,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url("127.0.0.1:8545".to_string())
        .with_sender(sender);

    let spinner = Spinner::new(MSG_BUILDING_ECOSYSTEM_CONTRACTS_SPINNER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let script_output = DeployL1Output::read(
        shell,
        DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.output(&config.link_to_code),
    )?;
    let mut contracts_config = ContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);

    Ok(contracts_config)
}
