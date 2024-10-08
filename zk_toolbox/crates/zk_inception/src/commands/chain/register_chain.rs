use anyhow::Context;
use common::{
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_CHAIN_REGISTERED, MSG_L1_SECRETS_MUST_BE_PRESENTED,
        MSG_REGISTERING_CHAIN_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let mut contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config()?;
    let l1_rpc_url = secrets
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    register_chain(
        shell,
        args,
        &ecosystem_config,
        &chain_config,
        &mut contracts,
        l1_rpc_url,
        None,
        true,
    )
    .await?;
    contracts.save_with_base_path(shell, chain_config.configs)?;
    spinner.finish();
    logger::success(MSG_CHAIN_REGISTERED);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn register_chain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    contracts: &mut ContractsConfig,
    l1_rpc_url: String,
    sender: Option<String>,
    broadcast: bool,
) -> anyhow::Result<()> {
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let deploy_config = RegisterChainL1Config::new(chain_config, contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url);

    if broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    contracts.set_chain_contracts(&register_chain_output);
    Ok(())
}
