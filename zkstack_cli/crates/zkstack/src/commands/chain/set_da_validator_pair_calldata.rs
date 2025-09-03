use anyhow::Context;
use clap::Parser;
use ethers::providers::Middleware;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{ethereum::get_ethers_provider, logger};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};
use zksync_types::{Address, L2_BRIDGEHUB_ADDRESS};

use super::utils::display_admin_script_output;
use crate::{
    abi::BridgehubAbi,
    admin_functions::{set_da_validator_pair, set_da_validator_pair_via_gateway, AdminScriptMode},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct SetDAValidatorPairCalldataArgs {
    /// The address of the DA validator be to used on the settlement layer.
    /// It is a contract that is deployed on the corresponding settlement layer (either L1 or GW).
    pub sl_da_validator: Address,

    /// Gateway transaction filterer
    pub l2_da_validator: Address,

    /// Bridgehub address
    pub bridgehub_address: Address,

    /// The address of the ZK chain diamond proxy
    pub chain_id: u64,

    pub l1_rpc_url: String,

    /// In order to override the current settlement layer
    pub explicit_settlement_layer_chain_id: Option<u64>,

    /// Max L1 gas price to be used for L1->GW transaction (in case the chain is settling on top of ZK Gateway)
    #[clap(
        long,
        help = "Max L1 gas price to be used for L1->GW transaction (in case the chain is settling on top of ZK Gateway)"
    )]
    pub max_l1_gas_price: Option<u64>,
    /// The refund recipient for L1->GW transaction (in case the chain is settling on top of ZK Gateway)
    #[clap(
        long,
        help = "The refund recipient for L1->GW transaction (in case the chain is settling on top of ZK Gateway)"
    )]
    pub refund_recipient: Option<Address>,
    /// The ZK Gateway RPC URL (only used in case the chain is settling on top of ZK Gateway)
    #[clap(
        long,
        help = "The ZK Gateway RPC URL (only used in case the chain is settling on top of ZK Gateway)"
    )]
    pub gw_rpc_url: Option<String>,
}

pub async fn run(shell: &Shell, args: SetDAValidatorPairCalldataArgs) -> anyhow::Result<()> {
    let chain_config = zkstack_cli_config::ZkStackConfig::current_chain(shell)
        .context("Failed to load the current chain configuration")?;
    let l1_provider = get_ethers_provider(&args.l1_rpc_url)?;
    let l1_bridgehub = BridgehubAbi::new(args.bridgehub_address, l1_provider.clone());
    let l1_chain_id = l1_provider.get_chainid().await?.as_u64();

    let used_settlement_layer = if let Some(layer) = args.explicit_settlement_layer_chain_id {
        layer
    } else {
        let current_sl_chain_id = l1_bridgehub
            .settlement_layer(args.chain_id.into())
            .await?
            .as_u64();

        let chain_description_text = if current_sl_chain_id == l1_chain_id {
            format!("chain_id {l1_chain_id} (Ethereum L1)")
        } else {
            format!("chain_id {current_sl_chain_id} (ZK Gateway)")
        };

        logger::warn(format!("Explicit settlement layer not provided! We will use the current settlement layer retrieved from L1: {chain_description_text}"));

        current_sl_chain_id
    };

    let result = if l1_chain_id == used_settlement_layer {
        set_da_validator_pair(
            shell,
            &Default::default(),
            &chain_config.path_to_foundry_scripts(),
            AdminScriptMode::OnlySave,
            args.chain_id,
            args.bridgehub_address,
            args.sl_da_validator,
            args.l2_da_validator,
            args.l1_rpc_url,
        )
        .await?
    } else {
        let gw_rpc_url = args
            .gw_rpc_url
            .context("Must provide `--gw-rpc-url` when preparing L1->GW transaction")?;
        let gw_bridgehub =
            BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, get_ethers_provider(&gw_rpc_url)?);
        let chain_diamond_proxy_on_gateway =
            gw_bridgehub.get_zk_chain(args.chain_id.into()).await?;

        if chain_diamond_proxy_on_gateway == Address::zero() {
            anyhow::bail!("The chain does not settle on GW yet, the address is known");
        }
        let contracts_foundry_path = ZkStackConfig::from_file(shell)?.path_to_foundry_scripts();

        let output = set_da_validator_pair_via_gateway(
            shell,
            &Default::default(),
            &contracts_foundry_path,
            AdminScriptMode::OnlySave,
            args.bridgehub_address,
            args.max_l1_gas_price
                .context("Must provide `--max-l1-gas-price` when preparing L1->GW transaction")?
                .into(),
            args.chain_id,
            used_settlement_layer,
            args.sl_da_validator,
            args.l2_da_validator,
            chain_diamond_proxy_on_gateway,
            args.refund_recipient
                .context("Must provide `--refund-recipient` when preparing L1->GW transaction")?,
            args.l1_rpc_url,
        )
        .await?;

        logger::warn("IMPORTANT: The prepared calldata only works for chains that settle on top of Gateway, if you wish to prepare calldata to set DA pair after migration to L1, please provide the `--expicit-settlement-layer-chain-id` param");

        output
    };

    display_admin_script_output(result);

    Ok(())
}
