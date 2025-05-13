use anyhow::Context;
use clap::Parser;
use ethers::utils::hex;
use serde::Deserialize;
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::get_ethers_provider, forge::ForgeScriptArgs, logger, spinner::Spinner,
};
use zkstack_cli_config::EcosystemConfig;
use zksync_basic_types::Address;
use zksync_web3_decl::jsonrpsee::core::Serialize;

use crate::{
    abi::BridgehubAbi,
    admin_functions::{set_da_validator_pair, set_da_validator_pair_via_gateway, AdminScriptMode},
    commands::chain::{
        gateway::gateway_common::get_settlement_layer_address_from_gw,
        utils::get_default_foundry_path,
    },
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        MSG_GOT_SETTLEMENT_LAYER_ADDRESS_FROM_GW, MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER,
    },
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct SetDAValidatorPairArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    /// Max L1 gas price to be used for L1->GW transaction (in case the chain is settling on top of ZK Gateway)
    pub max_l1_gas_price: Option<u64>,

    /// The address of the DA validator be to used on the settlement layer.
    /// It is a contract that is deployed on the corresponding settlement layer (either L1 or GW).
    pub l1_da_validator: Address,
}

pub async fn run(args: SetDAValidatorPairArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let gateway_url = chain_config.get_secrets_config().await?.gateway_rpc_url();
    let chain_id = chain_config.chain_id.as_u64();

    let l2_da_validator_address = contracts_config
        .l2
        .da_validator_addr
        .context("da_validator_addr")?;

    let spinner = Spinner::new(MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER);

    if let Ok(gateway_url) = gateway_url {
        let diamond_proxy_address =
            get_settlement_layer_address_from_gw(gateway_url.clone(), chain_id).await?;
        logger::note(
            MSG_GOT_SETTLEMENT_LAYER_ADDRESS_FROM_GW,
            format!("{}", hex::encode(diamond_proxy_address)),
        );

        let l1_provider =
            get_ethers_provider(&chain_config.get_secrets_config().await?.l1_rpc_url()?)?;
        let l1_bridgehub = BridgehubAbi::new(
            contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
            l1_provider.clone(),
        );
        let gw_chain_id = l1_bridgehub
            .settlement_layer(chain_id.into())
            .await?
            .as_u64();

        set_da_validator_pair_via_gateway(
            shell,
            &args.forge_args.clone(),
            &get_default_foundry_path(shell)?,
            AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
            contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
            args.max_l1_gas_price
                .context("Must provide `--max-l1-gas-price` when preparing L1->GW transaction")?
                .into(),
            chain_id,
            gw_chain_id,
            args.l1_da_validator,
            l2_da_validator_address,
            diamond_proxy_address,
            Address::zero(),
            gateway_url,
        )
        .await?;
    } else {
        let diamond_proxy_address = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;
        let l1_rpc_url: String = chain_config
            .get_secrets_config()
            .await?
            .l1_rpc_url()?
            .to_string();

        set_da_validator_pair(
            shell,
            &args.forge_args.clone(),
            &get_default_foundry_path(shell)?,
            AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
            chain_id,
            diamond_proxy_address,
            args.l1_da_validator,
            l2_da_validator_address,
            l1_rpc_url,
        )
        .await?;
    }

    spinner.finish();

    logger::note(
        MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        format!(
            "{} {}",
            hex::encode(args.l1_da_validator),
            hex::encode(l2_da_validator_address)
        ),
    );

    Ok(())
}
