use anyhow::{bail, Context};
use xshell::Shell;
use zkstack_cli_common::{logger, spinner::Spinner};
use zkstack_cli_config::{
    create_local_configs_dir, create_wallets, get_default_era_chain_id,
    traits::SaveConfigWithBasePath, EcosystemConfig, EcosystemConfigFromFileError, ZkStackConfig,
    ZkStackConfigTrait,
};

use crate::{
    commands::{
        chain::create_chain_inner,
        containers::{initialize_docker, start_containers},
        ecosystem::{
            args::create::EcosystemCreateArgs,
            create_configs::{
                create_apps_config, create_erc20_deployment_config,
                create_initial_deployments_config,
            },
        },
    },
    messages::{
        msg_created_ecosystem, MSG_ARGS_VALIDATOR_ERR, MSG_CREATING_DEFAULT_CHAIN_SPINNER,
        MSG_CREATING_ECOSYSTEM, MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER,
        MSG_ECOSYSTEM_ALREADY_EXISTS_ERR, MSG_ECOSYSTEM_CONFIG_INVALID_ERR, MSG_SELECTED_CONFIG,
        MSG_STARTING_CONTAINERS_SPINNER,
    },
    utils::link_to_code::resolve_link_to_code,
};

pub async fn run(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    match ZkStackConfig::ecosystem(shell) {
        Ok(_) => bail!(MSG_ECOSYSTEM_ALREADY_EXISTS_ERR),
        Err(EcosystemConfigFromFileError::InvalidConfig { .. }) => {
            bail!(MSG_ECOSYSTEM_CONFIG_INVALID_ERR)
        }
        Err(EcosystemConfigFromFileError::NotExists { .. }) => create(args, shell).await?,
    };

    Ok(())
}

async fn create(args: EcosystemCreateArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args
        .fill_values_with_prompt(shell)
        .context(MSG_ARGS_VALIDATOR_ERR)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&args));
    logger::info(MSG_CREATING_ECOSYSTEM);

    let ecosystem_name = &args.ecosystem_name;
    shell.create_dir(ecosystem_name)?;
    shell.change_dir(ecosystem_name);

    let configs_path = create_local_configs_dir(shell, ".")?;

    let link_to_code = resolve_link_to_code(
        shell,
        &shell.current_dir(),
        args.link_to_code.clone(),
        args.update_submodules,
    )?;

    let spinner = Spinner::new(MSG_CREATING_INITIAL_CONFIGURATIONS_SPINNER);
    let chain_config = args.chain_config();
    let chains_path = shell.create_dir("chains")?;
    let default_chain_name = args.chain_args.chain_name.clone();

    create_initial_deployments_config(shell, &configs_path)?;
    create_erc20_deployment_config(shell, &configs_path)?;
    create_apps_config(shell, &configs_path)?;

    let ecosystem_config = EcosystemConfig::new(
        ecosystem_name.clone(),
        args.l1_network,
        link_to_code,
        None,
        chains_path.clone(),
        configs_path,
        default_chain_name.clone(),
        get_default_era_chain_id(),
        chain_config.prover_version,
        args.wallet_creation,
        shell.clone().into(),
    );

    // Use 0 id for ecosystem  wallets
    create_wallets(
        shell,
        &ecosystem_config.config,
        &ecosystem_config.link_to_code(),
        0,
        args.wallet_creation,
        args.wallet_path,
    )?;
    ecosystem_config.save_with_base_path(shell, ".")?;
    spinner.finish();

    let spinner = Spinner::new(MSG_CREATING_DEFAULT_CHAIN_SPINNER);
    create_chain_inner(chain_config, &ecosystem_config, shell).await?;
    spinner.finish();

    if args.start_containers {
        let spinner = Spinner::new(MSG_STARTING_CONTAINERS_SPINNER);
        initialize_docker(shell, &ecosystem_config.link_to_code())?;
        start_containers(shell, false)?;
        spinner.finish();
    }

    logger::outro(msg_created_ecosystem(ecosystem_name));
    Ok(())
}
