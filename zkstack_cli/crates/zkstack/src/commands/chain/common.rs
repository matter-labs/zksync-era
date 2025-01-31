use zkstack_cli_common::spinner::Spinner;
use zkstack_cli_config::{ChainConfig, EcosystemConfig};
use zkstack_cli_types::{BaseToken, L1Network, WalletCreation};

use crate::{
    consts::AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
    messages::{MSG_DISTRIBUTING_ETH_SPINNER, MSG_MINT_BASE_TOKEN_SPINNER},
};

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
        if let Some(setter) = chain_wallets.token_multiplier_setter {
            addresses.push(setter.address)
        }
        zkstack_cli_common::ethereum::distribute_eth(
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
        zkstack_cli_common::ethereum::mint_token(
            wallets.governor,
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
