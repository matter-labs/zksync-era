use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, CoreContractsConfig, EcosystemConfig, ZkStackConfig,
    ZkStackConfigTrait,
};

use crate::{
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_CHAIN_REGISTERED, MSG_REGISTERING_CHAIN_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts = ecosystem_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    let contracts = register_chain(
        shell,
        args,
        &ecosystem_config,
        &chain_config,
        &contracts,
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
    contracts: &CoreContractsConfig,
    l1_rpc_url: String,
    sender: Option<String>,
    broadcast: bool,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.path_to_foundry_scripts());

    let deploy_config = RegisterChainL1Config::new(chain_config, contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
        .script(&REGISTER_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url);

    if broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(
            forge,
            Some(&config.get_wallets()?.governor),
            WalletOwner::Governor,
        )?;
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.path_to_foundry_scripts()),
    )?;
    let full_contracts =
        contracts.chain_contracts_from_output(&register_chain_output, chain_config);
    Ok(full_contracts)
}
