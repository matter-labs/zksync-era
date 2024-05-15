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
        update_paymaster, EcosystemConfig, HyperchainConfig, ReadConfig, SaveConfig,
    },
    consts::DEPLOY_PAYMASTER,
    forge_utils::fill_forge_private_key,
};

pub fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let hyperchain_name = global_config().hyperchain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file()?;
    let hyperchain_config = ecosystem_config
        .load_hyperchain(hyperchain_name)
        .context("Hyperchain not initialized. Please create a hyperchain first")?;
    deploy_paymaster(shell, &hyperchain_config, &ecosystem_config, args)
}

fn deploy_paymaster(
    shell: &Shell,
    hyperchain_config: &HyperchainConfig,
    ecosystem_config: &EcosystemConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let input = DeployPaymasterInput::new(hyperchain_config)?;
    let foundry_contracts_path = hyperchain_config.path_to_foundry();
    input.save(
        shell,
        DEPLOY_PAYMASTER.input(&hyperchain_config.link_to_code),
    )?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&DEPLOY_PAYMASTER.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(ecosystem_config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        hyperchain_config
            .get_wallets_config()?
            .governor_private_key(),
    )?;

    let spinner = Spinner::new("Deploying paymaster");
    forge.run(shell)?;
    spinner.finish();

    let output =
        DeployPaymasterOutput::read(DEPLOY_PAYMASTER.output(&hyperchain_config.link_to_code))?;

    update_paymaster(shell, hyperchain_config, &output)?;
    Ok(())
}
