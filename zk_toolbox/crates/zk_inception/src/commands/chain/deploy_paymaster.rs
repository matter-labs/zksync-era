use common::{
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use xshell::Shell;

use crate::forge_utils::check_the_balance;
use crate::{
    configs::{
        forge_interface::paymaster::{DeployPaymasterInput, DeployPaymasterOutput},
        update_paymaster, ChainConfig, EcosystemConfig, ReadConfig, SaveConfig,
    },
    consts::DEPLOY_PAYMASTER,
    forge_utils::fill_forge_private_key,
};

use super::init::load_global_config;

pub async fn run(
    args: ForgeScriptArgs,
    shell: &Shell,
    ecosystem_config: EcosystemConfig,
) -> anyhow::Result<()> {
    let chain_config = load_global_config(&ecosystem_config)?;
    deploy_paymaster(shell, &chain_config, &ecosystem_config, args).await
}

pub async fn deploy_paymaster(
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
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let output =
        DeployPaymasterOutput::read(shell, DEPLOY_PAYMASTER.output(&chain_config.link_to_code))?;

    update_paymaster(shell, chain_config, &output)?;
    Ok(())
}
