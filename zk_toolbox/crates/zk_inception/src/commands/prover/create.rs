use std::{path::PathBuf, str::FromStr};

use anyhow::{bail, Context};
use common::{git, logger, spinner::Spinner};
use config::{
    create_local_configs_dir, traits::SaveConfigWithBasePath, GeneralProverConfig,
    GeneralProverConfigFromFileError, ZKSYNC_ERA_GIT_REPO,
};
use xshell::Shell;

use crate::{
    commands::{
        containers::{initialize_docker, start_containers},
        prover::args::create::ProverCreateArgs,
    },
    messages::{
        msg_created_prover_subsystem, MSG_ARGS_VALIDATOR_ERR, MSG_CLONING_ERA_REPO_SPINNER,
        MSG_CREATING_ECOSYSTEM, MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER,
        MSG_ECOSYSTEM_ALREADY_EXISTS_ERR, MSG_ECOSYSTEM_CONFIG_INVALID_ERR, MSG_SELECTED_CONFIG,
        MSG_STARTING_CONTAINERS_SPINNER,
    },
};

pub fn run(args: ProverCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    match GeneralProverConfig::from_file(shell) {
        Ok(_) => bail!(MSG_ECOSYSTEM_ALREADY_EXISTS_ERR),
        Err(GeneralProverConfigFromFileError::InvalidConfig { .. }) => {
            bail!(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)
        }
        Err(GeneralProverConfigFromFileError::NotExists { .. }) => create(args, shell)?,
    };

    Ok(())
}

fn create(args: ProverCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args
        .fill_values_with_prompt(shell)
        .context(MSG_ARGS_VALIDATOR_ERR)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_ECOSYSTEM);

    let subsystem_name = &args.subsystem_name;
    shell.create_dir(subsystem_name)?;
    shell.change_dir(subsystem_name);

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

    let prover_config = GeneralProverConfig {
        name: subsystem_name.clone(),
        link_to_code: link_to_code.clone(),
        bellman_cuda_dir: None,
        config: configs_path,
        shell: shell.clone().into(),
    };

    prover_config.save_with_base_path(shell, ".")?;
    spinner.finish();

    if args.start_containers {
        let spinner = Spinner::new(MSG_STARTING_CONTAINERS_SPINNER);
        initialize_docker(shell, prover_config.link_to_code.clone())?;
        start_containers(shell, false)?;
        spinner.finish();
    }

    logger::outro(msg_created_prover_subsystem(subsystem_name));
    Ok(())
}
