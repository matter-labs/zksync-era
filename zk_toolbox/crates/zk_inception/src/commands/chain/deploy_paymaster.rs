use anyhow::Context;
use common::forge::{Forge, ForgeScriptArgs};
use config::{
    forge_interface::{
        paymaster::{DeployPaymasterInput, DeployPaymasterOutput},
        script_params::DEPLOY_PAYMASTER_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use xshell::Shell;

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let mut contracts = chain_config.get_contracts_config()?;
    deploy_paymaster(shell, &chain_config, &mut contracts, args, None, true).await?;
    contracts.save_with_base_path(shell, chain_config.configs)
}

pub async fn deploy_paymaster(
    shell: &Shell,
    chain_config: &ChainConfig,
    contracts_config: &mut ContractsConfig,
    forge_args: ForgeScriptArgs,
    sender: Option<String>,
    broadcast: bool,
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
        );

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(forge, Some(&chain_config.get_wallets_config()?.governor))?;
    }

    if broadcast {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let output = DeployPaymasterOutput::read(
        shell,
        DEPLOY_PAYMASTER_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;

    contracts_config.l2.testnet_paymaster_addr = output.paymaster;
    Ok(())
}
