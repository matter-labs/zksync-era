use xshell::Shell;
use zkstack_cli_common::forge::{Forge, ForgeScriptArgs};
use zkstack_cli_config::{
    forge_interface::{
        paymaster::{DeployPaymasterInput, DeployPaymasterOutput},
        script_params::DEPLOY_PAYMASTER_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, ZkStackConfig,
};

use crate::utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;
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
    let foundry_contracts_path = chain_config.path_to_l1_foundry();
    input.save(
        shell,
        DEPLOY_PAYMASTER_SCRIPT_PARAMS.input(&chain_config.path_to_l1_foundry()),
    )?;
    let secrets = chain_config.get_secrets_config().await?;

    let mut forge = Forge::new(&foundry_contracts_path)
        .script(&DEPLOY_PAYMASTER_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(secrets.l1_rpc_url()?);

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(
            forge,
            Some(&chain_config.get_wallets_config()?.governor),
            WalletOwner::Governor,
        )?;
    }

    if broadcast {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let output = DeployPaymasterOutput::read(
        shell,
        DEPLOY_PAYMASTER_SCRIPT_PARAMS.output(&chain_config.path_to_l1_foundry()),
    )?;

    contracts_config.l2.testnet_paymaster_addr = output.paymaster;
    Ok(())
}
