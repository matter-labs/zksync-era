use std::path::Path;

use anyhow::Context;
use common::{
    cmd::Cmd,
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use xshell::{cmd, Shell};

use crate::{
    configs::{
        forge_interface::initialize_bridges::{
            input::InitializeBridgeInput, output::InitializeBridgeOutput,
        },
        update_l2_shared_bridge, ChainConfig, EcosystemConfig, ReadConfig, SaveConfig,
    },
    consts::INITIALIZE_BRIDGES,
    forge_utils::fill_forge_private_key,
};

pub fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context("Chain not initialized. Please create a chain first")?;

    let spinner = Spinner::new("Initializing bridges");
    initialize_bridges(shell, &chain_config, &ecosystem_config, args)?;
    spinner.finish();

    Ok(())
}

pub fn initialize_bridges(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    build_l2_contracts(shell, &ecosystem_config.link_to_code)?;
    let input = InitializeBridgeInput::new(chain_config, ecosystem_config.era_chain_id)?;
    let foundry_contracts_path = chain_config.path_to_foundry();
    input.save(shell, INITIALIZE_BRIDGES.input(&chain_config.link_to_code))?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&INITIALIZE_BRIDGES.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(ecosystem_config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.governor_private_key(),
    )?;

    forge.run(shell)?;

    let output =
        InitializeBridgeOutput::read(shell, INITIALIZE_BRIDGES.output(&chain_config.link_to_code))?;

    update_l2_shared_bridge(shell, chain_config, &output)?;
    Ok(())
}

fn build_l2_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Cmd::new(cmd!(shell, "yarn l2 build")).run()
}
