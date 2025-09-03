use anyhow::Context;
use clap::Parser;
use ethers::utils::hex;
use serde::Deserialize;
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::get_ethers_provider, forge::ForgeScriptArgs, logger, spinner::Spinner,
};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};
use zksync_basic_types::Address;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_web3_decl::jsonrpsee::core::Serialize;

use crate::{
    abi::{BridgehubAbi, ZkChainAbi},
    admin_functions::{set_da_validator_pair, set_da_validator_pair_via_gateway, AdminScriptMode},
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_DA_VALIDATOR_PAIR_UPDATED_TO,
        MSG_GATEWAY_URL_MUST_BE_PRESET, MSG_GOT_SETTLEMENT_LAYER_ADDRESS_FROM_GW,
        MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER, MSG_USE_GATEWAY_HELP,
    },
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct SetDAValidatorPairArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    /// The address of the DA validator be to used on the settlement layer.
    /// It is a contract that is deployed on the corresponding settlement layer (either L1 or GW).
    pub l1_da_validator: Address,

    /// Max L1 gas price to be used for L1->GW transaction (in case the chain is settling on top of ZK Gateway)
    pub max_l1_gas_price: Option<u64>,
    #[clap(long, help = MSG_USE_GATEWAY_HELP)]
    pub gateway: bool,
}

pub async fn run(args: SetDAValidatorPairArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell).context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let chain_id = chain_config.chain_id.as_u64();

    let l2_da_validator_address = contracts_config
        .l2
        .da_validator_addr
        .context("da_validator_addr")?;
    let l1_rpc_url: String = chain_config
        .get_secrets_config()
        .await?
        .l1_rpc_url()?
        .to_string();

    let spinner = Spinner::new(MSG_UPDATING_DA_VALIDATOR_PAIR_SPINNER);

    if args.gateway {
        let gateway_url = chain_config
            .get_secrets_config()
            .await?
            .gateway_rpc_url()
            .expect(MSG_GATEWAY_URL_MUST_BE_PRESET);
        let gw_bridgehub =
            BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, get_ethers_provider(&gateway_url)?);
        let chain_diamond_proxy_on_gateway = gw_bridgehub.get_zk_chain(chain_id.into()).await?;

        if chain_diamond_proxy_on_gateway.is_zero() {
            anyhow::bail!("The chain does not settle on GW yet, the address is unknown");
        }
        logger::note(
            MSG_GOT_SETTLEMENT_LAYER_ADDRESS_FROM_GW,
            hex::encode(chain_diamond_proxy_on_gateway).to_string(),
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
        let refund_recipient = chain_config.get_wallets_config()?.governor.address;
        let contracts_foundry_path = ZkStackConfig::from_file(shell)?.path_to_l1_foundry();

        set_da_validator_pair_via_gateway(
            shell,
            &args.forge_args.clone(),
            &contracts_foundry_path,
            AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
            contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
            args.max_l1_gas_price
                .context("Must provide `--max-l1-gas-price` when preparing L1->GW transaction")?
                .into(),
            chain_id,
            gw_chain_id,
            args.l1_da_validator,
            l2_da_validator_address,
            chain_diamond_proxy_on_gateway,
            refund_recipient,
            l1_rpc_url,
        )
        .await?;

        // Wait for the transaction to be picked up on Gateway
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let zk_chain_abi = ZkChainAbi::new(
            chain_diamond_proxy_on_gateway,
            get_ethers_provider(&gateway_url)?,
        );
        let (l1_da_validator, l2_da_validator) =
            zk_chain_abi.get_da_validator_pair().call().await?;

        logger::note(
            "DA validator pair on Gateway:",
            format!(
                "L1: {}, L2: {}",
                hex::encode(l1_da_validator),
                hex::encode(l2_da_validator)
            ),
        );
    } else {
        let diamond_proxy_address = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;

        set_da_validator_pair(
            shell,
            &args.forge_args.clone(),
            &chain_config.path_to_l1_foundry(),
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
