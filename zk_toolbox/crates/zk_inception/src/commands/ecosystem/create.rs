use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail};
use common::{cmd::Cmd, logger, spinner::Spinner};
use xshell::{cmd, Shell};

use crate::{
    commands::{
        containers::start_containers, ecosystem::args::create::EcosystemCreateArgs,
        hyperchain::create_hyperchain_inner,
    },
    configs::{
        forge_interface::deploy_ecosystem::input::{
            Erc20DeploymentConfig, InitialDeploymentConfig,
        },
        EcosystemConfig, EcosystemConfigFromFileError, SaveConfig, SaveConfigWithComment,
    },
    consts::{
        CONFIG_NAME, DOCKER_COMPOSE_FILE, ERA_CHAIN_ID, ERC20_DEPLOYMENT_FILE,
        INITIAL_DEPLOYMENT_FILE, LOCAL_CONFIGS_PATH, WALLETS_FILE, ZKSYNC_ERA_GIT_REPO,
    },
    wallets::create_wallets,
};

pub fn run(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    match EcosystemConfig::from_file() {
        Ok(_) => bail!("Ecosystem already exists"),
        Err(EcosystemConfigFromFileError::InvalidConfig) => {
            bail!("Invalid ecosystem configuration")
        }
        Err(EcosystemConfigFromFileError::NotExists) => create(args, shell)?,
    };

    Ok(())
}

fn create(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    logger::note("Selected config:", &logger::object_to_string(&args));
    logger::info("Creating ecosystem");

    let ecosystem_name = &args.ecosystem_name;
    shell.create_dir(ecosystem_name)?;
    shell.change_dir(ecosystem_name);

    let configs_path = shell.create_dir(LOCAL_CONFIGS_PATH)?;

    let link_to_code = if args.link_to_code.is_empty() {
        let spinner = Spinner::new("Cloning zksync-era repository...");
        let link_to_code = clone_era_repo(shell)?;
        spinner.finish();
        link_to_code
    } else {
        PathBuf::from_str(&args.link_to_code)?
    };

    let spinner = Spinner::new("Creating initial configurations...");
    let hyperchain_config = args.hyperchain_config();
    let hyperchains_path = shell.create_dir("hyperchains")?;
    let default_hyperchain_name = args.hyperchain_args.hyperchain_name.clone();

    create_initial_deployments_config(shell, &configs_path)?;
    create_erc20_deployment_config(shell, &configs_path)?;

    let ecosystem_config = EcosystemConfig {
        name: ecosystem_name.clone(),
        l1_network: args.l1_network,
        link_to_code: link_to_code.clone(),
        hyperchains: hyperchains_path.clone(),
        config: configs_path,
        default_hyperchain: default_hyperchain_name.clone(),
        l1_rpc_url: args.l1_rpc_url,
        era_chain_id: ERA_CHAIN_ID,
        prover_version: hyperchain_config.prover_version,
    };

    create_wallets(
        shell,
        &ecosystem_config.config.join(WALLETS_FILE),
        &ecosystem_config.link_to_code,
        args.wallet_creation,
        args.wallet_path,
    )?;
    init_docker(shell, link_to_code.clone())?;
    ecosystem_config.save(shell, CONFIG_NAME)?;
    spinner.finish();

    let spinner = Spinner::new("Creating default hyperchain...");
    create_hyperchain_inner(hyperchain_config, &ecosystem_config, shell)?;
    spinner.finish();

    if args.start_containers {
        let spinner = Spinner::new("Starting containers...");
        start_containers(shell)?;
        spinner.finish();
    }

    logger::outro("Ecosystem created successfully");
    Ok(())
}

fn init_docker(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let docker_compose_file = link_to_code.join(DOCKER_COMPOSE_FILE);

    let docker_compose_text = shell.read_file(&docker_compose_file).map_err(|err| {
        anyhow!(
            "Failed to read docker compose file from {:?}: {}",
            &docker_compose_file,
            err
        )
    })?;
    let original_source = "./etc/reth/chaindata";
    let new_source = link_to_code.join(original_source);
    let new_source = new_source.to_str().unwrap();

    let data = docker_compose_text.replace(original_source, new_source);
    shell.write_file(DOCKER_COMPOSE_FILE, data)?;
    shell.create_dir("volumes")?;
    shell.create_dir("volumes/postgres")?;
    shell.create_dir("volumes/reth")?;
    shell.create_dir("volumes/reth/data")?;
    Ok(())
}

fn clone_era_repo(shell: &Shell) -> anyhow::Result<PathBuf> {
    Cmd::new(cmd!(
        shell,
        "git clone -b deniallugo-update-contracts1 --depth 1 --recurse-submodules
        --shallow-submodules {ZKSYNC_ERA_GIT_REPO}"
    ))
    .run()?;
    Ok(shell.current_dir().join("zksync-era"))
}

fn create_initial_deployments_config(
    shell: &Shell,
    hyperchain_configs_path: &Path,
) -> anyhow::Result<()> {
    let config = InitialDeploymentConfig::default();
    config.save_with_comment(shell, hyperchain_configs_path.join(INITIAL_DEPLOYMENT_FILE), "ATTENTION: This file contains sensible placeholders. Please check them and update with the desired values.")
}

fn create_erc20_deployment_config(
    shell: &Shell,
    hyperchain_configs_path: &Path,
) -> anyhow::Result<()> {
    let config = Erc20DeploymentConfig::default();
    config.save_with_comment(
        shell,
        hyperchain_configs_path.join(ERC20_DEPLOYMENT_FILE),
        "ATTENTION: This file should be filled with the desired ERC20 tokens to deploy.",
    )
}
