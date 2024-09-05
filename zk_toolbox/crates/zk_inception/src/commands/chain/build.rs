use anyhow::Context;
use common::{config::global_config, forge::Forge, git, logger, spinner::Spinner};
use config::{
    copy_configs,
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    set_l1_rpc_url,
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    update_from_chain_config, EcosystemConfig,
};
use xshell::Shell;

use crate::{
    commands::chain::args::build::BuildArgs,
    messages::{
        msg_initializing_chain, MSG_CHAIN_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_REGISTERING_CHAIN_SPINNER, MSG_SELECTED_CONFIG,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub(crate) async fn run(args: BuildArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let config = EcosystemConfig::from_file(shell)?;
    let chain_config = config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let args = args.fill_values_with_prompt(&chain_config);

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));
    logger::info(msg_initializing_chain(""));
    git::submodule_update(shell, config.link_to_code.clone())?;

    copy_configs(shell, &config.link_to_code, &chain_config.configs)?;

    let general_config = chain_config.get_general_config()?;
    general_config.save_with_base_path(shell, &chain_config.configs)?;

    let mut genesis_config = chain_config.get_genesis_config()?;
    update_from_chain_config(&mut genesis_config, &chain_config);
    genesis_config.save_with_base_path(shell, &chain_config.configs)?;

    // Copy ecosystem contracts
    let mut contracts_config = config.get_contracts_config()?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    let mut secrets = chain_config.get_secrets_config()?;
    set_l1_rpc_url(&mut secrets, args.l1_rpc_url.clone())?;
    secrets.save_with_base_path(shell, &chain_config.configs)?;

    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let deploy_config = RegisterChainL1Config::new(&chain_config, &contracts_config)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(
            &REGISTER_CHAIN_SCRIPT_PARAMS.script(),
            args.forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(args.l1_rpc_url.clone());

    forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    contracts_config.set_chain_contracts(&register_chain_output);

    contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    spinner.finish();

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}
