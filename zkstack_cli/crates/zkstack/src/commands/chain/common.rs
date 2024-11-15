use common::spinner::Spinner;
use config::{ChainConfig, WalletsConfig};
use types::{BaseToken, L1Network, WalletCreation};

use crate::{
    consts::AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
    messages::{MSG_DISTRIBUTING_ETH_SPINNER, MSG_MINT_BASE_TOKEN_SPINNER},
};

// Distribute eth to the chain wallets for localhost environment
pub async fn distribute_eth(
    chain_config: &ChainConfig,
    l1_rpc_url: String,
    wallets: &WalletsConfig,
) -> anyhow::Result<()> {
    if chain_config.wallet_creation != WalletCreation::Localhost
        || chain_config.l1_network != L1Network::Localhost
    {
        return Ok(());
    }

    let spinner = Spinner::new(MSG_DISTRIBUTING_ETH_SPINNER);
    let chain_wallets = chain_config.get_wallets_config()?;
    let mut addresses = vec![
        chain_wallets.operator.address,
        chain_wallets.blob_operator.address,
        chain_wallets.governor.address,
    ];
    if let Some(deployer) = chain_wallets.deployer {
        addresses.push(deployer.address);
    }
    if let Some(setter) = chain_wallets.token_multiplier_setter {
        addresses.push(setter.address);
    }
    common::ethereum::distribute_eth(
        wallets.operator.clone(),
        addresses,
        l1_rpc_url,
        chain_config.l1_network.chain_id(),
        AMOUNT_FOR_DISTRIBUTION_TO_WALLETS,
    )
    .await?;
    spinner.finish();

    Ok(())
}

pub async fn mint_base_token(
    chain_config: &ChainConfig,
    l1_rpc_url: String,
    wallets: &WalletsConfig,
) -> anyhow::Result<()> {
    if chain_config.wallet_creation != WalletCreation::Localhost
        || chain_config.l1_network != L1Network::Localhost
        || chain_config.base_token == BaseToken::eth()
    {
        return Ok(());
    }

    let spinner = Spinner::new(MSG_MINT_BASE_TOKEN_SPINNER);
    let chain_wallets = chain_config.get_wallets_config()?;
    let base_token = &chain_config.base_token;
    let addresses = vec![wallets.governor.address, chain_wallets.governor.address];
    let amount = AMOUNT_FOR_DISTRIBUTION_TO_WALLETS * base_token.nominator as u128
        / base_token.denominator as u128;
    common::ethereum::mint_token(
        wallets.governor.clone(),
        base_token.address,
        addresses,
        l1_rpc_url,
        chain_config.l1_network.chain_id(),
        amount,
    )
    .await?;
    spinner.finish();

    Ok(())
}
