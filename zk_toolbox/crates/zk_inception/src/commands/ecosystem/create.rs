use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::bail;
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::{
    create_local_configs_dir, create_wallets, get_default_era_chain_id,
    traits::SaveConfigWithBasePath, EcosystemConfig, EcosystemConfigFromFileError,
    ZKSYNC_ERA_GIT_REPO,
};
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
    consts::BELLMAN_CUDA_DIR,
    messages::{
        MSG_CLONING_ERA_REPO_SPINNER, MSG_CREATED_ECOSYSTEM, MSG_CREATING_DEFAULT_CHAIN_SPINNER,
        MSG_CREATING_ECOSYSTEM, MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER,
        MSG_ECOSYSTEM_ALREADY_EXISTS_ERR, MSG_ECOSYSTEM_CONFIG_INVALID_ERR, MSG_SELECTED_CONFIG,
        MSG_STARTING_CONTAINERS_SPINNER,
    },
};

pub fn run(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    match EcosystemConfig::from_file(shell) {
        Ok(_) => bail!(MSG_ECOSYSTEM_ALREADY_EXISTS_ERR),
        Err(EcosystemConfigFromFileError::InvalidConfig { .. }) => {
            bail!(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)
        }
        Err(EcosystemConfigFromFileError::NotExists) => create(args, shell)?,
    };

    Ok(())
}

fn create(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_ECOSYSTEM);

    let ecosystem_name = &args.ecosystem_name;
    shell.create_dir(ecosystem_name)?;
    shell.change_dir(ecosystem_name);

    let configs_path = create_local_configs_dir(shell, ".")?;

    let link_to_code = if args.link_to_code.is_empty() {
        let spinner = Spinner::new(MSG_CLONING_ERA_REPO_SPINNER);
        let link_to_code = clone_era_repo(shell)?;
        spinner.finish();
        link_to_code
    } else {
        let path = PathBuf::from_str(&args.link_to_code)?;
        update_submodules_recursive(shell, &path)?;
        path
    };

    let spinner = Spinner::new(MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER);
    let chain_config = args.chain_config();
    let chains_path = shell.create_dir("chains")?;
    let default_chain_name = args.chain_args.chain_name.clone();

    create_initial_deployments_config(shell, &configs_path)?;
    create_erc20_deployment_config(shell, &configs_path)?;

    let ecosystem_config = EcosystemConfig {
        name: ecosystem_name.clone(),
        l1_network: args.l1_network,
        link_to_code: link_to_code.clone(),
        bellman_cuda_dir: Some(shell.current_dir().join(BELLMAN_CUDA_DIR)),
        chains: chains_path.clone(),
        config: configs_path,
        era_chain_id: get_default_era_chain_id(),
        default_chain: default_chain_name.clone(),
        prover_version: chain_config.prover_version,
        wallet_creation: args.wallet_creation,
        shell: shell.clone().into(),
    };

    // Use 0 id for ecosystem  wallets
    create_wallets(
        shell,
        &ecosystem_config.config,
        &ecosystem_config.link_to_code,
        0,
        args.wallet_creation,
        args.wallet_path,
    )?;
    ecosystem_config.save_with_base_path(shell, ".")?;
    spinner.finish();

    let spinner = Spinner::new(MSG_CREATING_DEFAULT_CHAIN_SPINNER);
    create_chain_inner(chain_config, &ecosystem_config, shell)?;
    spinner.finish();

    if args.start_containers {
        let spinner = Spinner::new(MSG_STARTING_CONTAINERS_SPINNER);
        initialize_docker(shell, &ecosystem_config)?;
        start_containers(shell)?;
        spinner.finish();
    }

    logger::outro(MSG_CREATED_ECOSYSTEM);
    Ok(())
}

fn clone_era_repo(shell: &Shell) -> anyhow::Result<PathBuf> {
    Cmd::new(cmd!(
        shell,
        "git clone --recurse-submodules {ZKSYNC_ERA_GIT_REPO}"
    ))
    .run()?;
    Ok(shell.current_dir().join("zksync-era"))
}

fn update_submodules_recursive(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    Cmd::new(cmd!(
        shell,
        "git submodule update --init --recursive
"
    ))
    .run()?;
    Ok(())
}
