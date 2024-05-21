use std::{path::PathBuf, str::FromStr};

use anyhow::bail;
use common::{cmd::Cmd, logger, spinner::Spinner};
use xshell::{cmd, Shell};

use crate::{
    commands::{
        chain::create_chain_inner,
        containers::{initialize_docker, start_containers},
        ecosystem::{
            args::create::EcosystemCreateArgs,
            create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
        },
    },
    configs::{EcosystemConfig, EcosystemConfigFromFileError, SaveConfig},
    consts::{CONFIG_NAME, ERA_CHAIN_ID, LOCAL_CONFIGS_PATH, WALLETS_FILE, ZKSYNC_ERA_GIT_REPO},
    wallets::create_wallets,
};

pub fn run(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    match EcosystemConfig::from_file(shell) {
        Ok(_) => bail!("Ecosystem already exists"),
        Err(EcosystemConfigFromFileError::InvalidConfig { .. }) => {
            bail!("Invalid ecosystem configuration")
        }
        Err(EcosystemConfigFromFileError::NotExists) => create(args, shell)?,
    };

    Ok(())
}

fn create(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    logger::note("Selected config:", logger::object_to_string(&args));
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
    let chain_config = args.chain_config();
    let chains_path = shell.create_dir("chains")?;
    let default_chain_name = args.chain_args.chain_name.clone();

    create_initial_deployments_config(shell, &configs_path)?;
    create_erc20_deployment_config(shell, &configs_path)?;

    let ecosystem_config = EcosystemConfig {
        name: ecosystem_name.clone(),
        l1_network: args.l1_network,
        link_to_code: link_to_code.clone(),
        chains: chains_path.clone(),
        config: configs_path,
        default_chain: default_chain_name.clone(),
        l1_rpc_url: args.l1_rpc_url,
        era_chain_id: ERA_CHAIN_ID,
        prover_version: chain_config.prover_version,
        wallet_creation: args.wallet_creation,
        shell: shell.clone().into(),
    };

    // Use 0 id for ecosystem  wallets
    create_wallets(
        shell,
        &ecosystem_config.config.join(WALLETS_FILE),
        &ecosystem_config.link_to_code,
        0,
        args.wallet_creation,
        args.wallet_path,
    )?;
    ecosystem_config.save(shell, CONFIG_NAME)?;
    spinner.finish();

    let spinner = Spinner::new("Creating default chain...");
    create_chain_inner(chain_config, &ecosystem_config, shell)?;
    spinner.finish();

    if args.start_containers {
        let spinner = Spinner::new("Starting containers...");
        initialize_docker(shell, &ecosystem_config)?;
        start_containers(shell)?;
        spinner.finish();
    }

    logger::outro("Ecosystem created successfully");
    Ok(())
}

fn clone_era_repo(shell: &Shell) -> anyhow::Result<PathBuf> {
    Cmd::new(cmd!(
        shell,
        "git clone -b deniallugo-update-contracts1 --recurse-submodules {ZKSYNC_ERA_GIT_REPO}"
    ))
    .run()?;
    Ok(shell.current_dir().join("zksync-era"))
}
