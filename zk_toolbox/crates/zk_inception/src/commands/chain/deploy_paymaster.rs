use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use xshell::Shell;

use crate::{
    configs::{
        forge_interface::paymaster::{DeployPaymasterInput, DeployPaymasterOutput},
        update_paymaster, ChainConfig, EcosystemConfig, ReadConfig, SaveConfig,
    },
    consts::DEPLOY_PAYMASTER,
    forge_utils::fill_forge_private_key,
};

pub fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context("Chain not initialized. Please create a chain first")?;
    deploy_paymaster(shell, &chain_config, &ecosystem_config, args)
}

pub fn deploy_paymaster(
    shell: &Shell,
    chain_config: &ChainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let input = DeployPaymasterInput::new(chain_config)?;
    let foundry_contracts_path = chain_config.path_to_foundry();
    input.save(shell, DEPLOY_PAYMASTER.input(&chain_config.link_to_code))?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&DEPLOY_PAYMASTER.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(ecosystem_config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        chain_config.get_wallets_config()?.governor_private_key(),
    )?;

    let spinner = Spinner::new("Deploying paymaster");
    forge.run(shell)?;
    spinner.finish();

    let output =
        DeployPaymasterOutput::read(shell, DEPLOY_PAYMASTER.output(&chain_config.link_to_code))?;

    update_paymaster(shell, chain_config, &output)?;
    Ok(())
}
