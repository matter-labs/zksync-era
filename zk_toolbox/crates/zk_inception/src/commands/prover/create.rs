use std::{path::PathBuf, str::FromStr};

use anyhow::{bail, Context};
use common::{git, logger, spinner::Spinner};
use config::{create_local_configs_dir, create_wallets, get_default_era_chain_id, traits::SaveConfigWithBasePath, EcosystemConfig, EcosystemConfigFromFileError, ZKSYNC_ERA_GIT_REPO, ProverConfig, GeneralProverConfig};
use xshell::Shell;

use crate::{
    commands::{
        chain::create_chain_inner,
        containers::{initialize_docker, start_containers},
        prover::args::create::ProverCreateArgs,
    },
    messages::{
        msg_created_ecosystem, MSG_ARGS_VALIDATOR_ERR, MSG_CLONING_ERA_REPO_SPINNER,
        MSG_CREATING_DEFAULT_CHAIN_SPINNER, MSG_CREATING_ECOSYSTEM,
        MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER, MSG_ECOSYSTEM_ALREADY_EXISTS_ERR,
        MSG_ECOSYSTEM_CONFIG_INVALID_ERR, MSG_SELECTED_CONFIG, MSG_STARTING_CONTAINERS_SPINNER,
    },
};

pub fn run(args: ProverCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    match ProverConfig::from_file(shell) {
        Ok(_) => bail!(MSG_ECOSYSTEM_ALREADY_EXISTS_ERR),
        Err(EcosystemConfigFromFileError::InvalidConfig { .. }) => {
            bail!(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)
        }
        Err(EcosystemConfigFromFileError::NotExists { .. }) => create(args, shell)?,
    };

    Ok(())
}

fn create(args: ProverCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args
        .fill_values_with_prompt(shell)
        .context(MSG_ARGS_VALIDATOR_ERR)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_ECOSYSTEM);

    let ecosystem_name = &args.ecosystem_name;
    shell.create_dir(ecosystem_name)?;
    shell.change_dir(ecosystem_name);

    let configs_path = create_local_configs_dir(shell, ".")?;

    let link_to_code = if args.link_to_code.is_empty() {
        let spinner = Spinner::new(MSG_CLONING_ERA_REPO_SPINNER);
        let link_to_code = git::clone(
            shell,
            shell.current_dir(),
            ZKSYNC_ERA_GIT_REPO,
            "zksync-era",
        )?;
        spinner.finish();
        link_to_code
    } else {
        let path = PathBuf::from_str(&args.link_to_code)?;
        git::submodule_update(shell, path.clone())?;
        path
    };

    let spinner = Spinner::new(MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER);
    let chain_config = args.chain_config();
    let chains_path = shell.create_dir("chains")?;

    let prover_config = GeneralProverConfig {
        name: ecosystem_name.clone(),
        link_to_code: link_to_code.clone(),
        bellman_cuda_dir: None,
        config: configs_path,
        shell: shell.clone().into(),
    };

    ecosystem_config.save_with_base_path(shell, ".")?;
    spinner.finish();

    let spinner = Spinner::new(MSG_CREATING_DEFAULT_CHAIN_SPINNER);
    create_chain_inner(chain_config, &ecosystem_config, shell)?;
    spinner.finish();

    if args.start_containers {
        let spinner = Spinner::new(MSG_STARTING_CONTAINERS_SPINNER);
        initialize_docker(shell, &ecosystem_config)?;
        start_containers(shell, false)?;
        spinner.finish();
    }

    logger::outro(msg_created_ecosystem(ecosystem_name));
    Ok(())
}
