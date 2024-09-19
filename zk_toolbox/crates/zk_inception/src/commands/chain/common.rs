use common::{
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, SaveConfig},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use types::{BaseToken, L1Network, WalletCreation};
use xshell::Shell;

use crate::{
    consts::AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
    messages::{MSG_DISTRIBUTING_ETH_SPINNER, MSG_MINT_BASE_TOKEN_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key},
};

#[allow(clippy::too_many_arguments)]
pub async fn register_chain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    contracts: &mut ContractsConfig,
    l1_rpc_url: String,
    sender: Option<String>,
    broadcast: bool,
) -> anyhow::Result<()> {
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let deploy_config = RegisterChainL1Config::new(chain_config, contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url);

    if broadcast {
        forge = forge.with_broadcast();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    contracts.set_chain_contracts(&register_chain_output);
    Ok(())
}

// Distribute eth to the chain wallets for localhost environment
pub async fn distribute_eth(
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    if chain_config.wallet_creation == WalletCreation::Localhost
        && ecosystem_config.l1_network == L1Network::Localhost
    {
        let spinner = Spinner::new(MSG_DISTRIBUTING_ETH_SPINNER);
        let wallets = ecosystem_config.get_wallets()?;
        let chain_wallets = chain_config.get_wallets_config()?;
        let mut addresses = vec![
            chain_wallets.operator.address,
            chain_wallets.blob_operator.address,
            chain_wallets.governor.address,
        ];
        if let Some(deployer) = chain_wallets.deployer {
            addresses.push(deployer.address)
        }
        common::ethereum::distribute_eth(
            wallets.operator,
            addresses,
            l1_rpc_url,
            ecosystem_config.l1_network.chain_id(),
            AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
        )
        .await?;
        spinner.finish();
    }
    Ok(())
}

pub async fn mint_base_token(
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    if chain_config.wallet_creation == WalletCreation::Localhost
        && ecosystem_config.l1_network == L1Network::Localhost
        && chain_config.base_token != BaseToken::eth()
    {
        let spinner = Spinner::new(MSG_MINT_BASE_TOKEN_SPINNER);
        let wallets = ecosystem_config.get_wallets()?;
        let chain_wallets = chain_config.get_wallets_config()?;
        let base_token = &chain_config.base_token;
        let addresses = vec![wallets.governor.address, chain_wallets.governor.address];
        let amount = AMOUNT_FOR_DISTRIBUTION_TO_WALLETS * base_token.nominator as u128
            / base_token.denominator as u128;
        common::ethereum::mint_token(
            wallets.operator,
            base_token.address,
            addresses,
            l1_rpc_url,
            ecosystem_config.l1_network.chain_id(),
            amount,
        )
        .await?;
        spinner.finish();
    }
    Ok(())
}
