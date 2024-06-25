use std::path::Path;

use anyhow::Context;
use common::{
    cmd::Cmd,
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{
    forge_interface::{
        initialize_bridges::{input::InitializeBridgeInput, output::InitializeBridgeOutput},
        script_params::INITIALIZE_BRIDGES_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig},
    ChainConfig, EcosystemConfig,
};
use xshell::{cmd, Shell};

use crate::{
    config_manipulations::update_l2_shared_bridge,
    forge_utils::{check_the_balance, fill_forge_private_key},
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_INITIALIZING_BRIDGES_SPINNER},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let spinner = Spinner::new(MSG_INITIALIZING_BRIDGES_SPINNER);
    initialize_bridges(shell, &chain_config, &ecosystem_config, args).await?;
    spinner.finish();

    Ok(())
}

pub async fn initialize_bridges(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    build_l2_contracts(shell, &ecosystem_config.link_to_code)?;
    let input = InitializeBridgeInput::new(chain_config, ecosystem_config.era_chain_id)?;
    let foundry_contracts_path = chain_config.path_to_foundry();
    let secrets = chain_config.get_secrets_config()?;
    input.save(
        shell,
        INITIALIZE_BRIDGES_SCRIPT_PARAMS.input(&chain_config.link_to_code),
    )?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(
            &INITIALIZE_BRIDGES_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(secrets.l1.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.governor_private_key(),
    )?;

    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let output = InitializeBridgeOutput::read(
        shell,
        INITIALIZE_BRIDGES_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;

    update_l2_shared_bridge(shell, chain_config, &output)?;
    Ok(())
}

fn build_l2_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l2_contracts"));
    Cmd::new(cmd!(shell, "forge build --zksync")).run()
}
