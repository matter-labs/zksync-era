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
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::messages::MSG_L1_SECRETS_MUST_BE_PRESENTED;
use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_DEPLOYING_PAYMASTER},
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let mut contracts = chain_config.get_contracts_config()?;
    deploy_paymaster(shell, &chain_config, &mut contracts, args).await?;
    contracts.save_with_base_path(shell, chain_config.configs)
}

pub async fn deploy_paymaster(
    shell: &Shell,
    chain_config: &ChainConfig,
    contracts_config: &mut ContractsConfig,
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
        .with_rpc_url(
            secrets
                .l1
                .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
                .l1_rpc_url
                .expose_str()
                .to_string(),
        )
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

    contracts_config.l2.testnet_paymaster_addr = output.paymaster;
    Ok(())
}
