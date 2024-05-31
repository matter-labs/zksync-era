use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{
    forge_interface::{
        paymaster::{DeployPaymasterInput, DeployPaymasterOutput},
        script_params::DEPLOY_PAYMASTER_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig},
    ChainConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::{
    config_manipulations::update_paymaster,
    forge_utils::{check_the_balance, fill_forge_private_key},
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_DEPLOYING_PAYMASTER},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    deploy_paymaster(shell, &chain_config, args).await
}

pub async fn deploy_paymaster(
    shell: &Shell,
    chain_config: &ChainConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<()> {
    let input = DeployPaymasterInput::new(chain_config)?;
    let foundry_contracts_path = chain_config.path_to_foundry();
    input.save(
        shell,
        DEPLOY_PAYMASTER_SCRIPT_PARAMS.input(&chain_config.link_to_code),
    )?;
    let secrets = chain_config.get_secrets_config()?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&DEPLOY_PAYMASTER_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(secrets.l1.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        chain_config.get_wallets_config()?.governor_private_key(),
    )?;

    let spinner = Spinner::new(MSG_DEPLOYING_PAYMASTER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let output = DeployPaymasterOutput::read(
        shell,
        DEPLOY_PAYMASTER_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;

    update_paymaster(shell, chain_config, &output)?;
    Ok(())
}
