use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::parse_abi,
    contract::BaseContract,
    providers::{Http, Middleware, Provider},
    types::Bytes,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
    zks_provider::ZKSProvider,
};
use zkstack_cli_config::{
    forge_interface::{
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::GATEWAY_PREPARATION,
    },
    traits::{ReadConfig, SaveConfig},
    EcosystemConfig,
};
use zksync_basic_types::{L2ChainId, H256, U256, U64};
use zksync_web3_decl::client::{Client, L2};

use crate::{
    accept_ownership::set_da_validator_pair,
    commands::chain::init::get_l1_da_validator,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_DA_PAIR_REGISTRATION_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateFromGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
}

lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function startMigrateChainFromGateway(address chainAdmin,address accessControlRestriction,address l2ChainAdmin,uint256 chainId) public",
            "function finishMigrateChainFromGateway(uint256 migratingChainId,uint256 gatewayChainId,uint256 l2BatchNumber,uint256 l2MessageIndex,uint16 l2TxNumberInBatch,bytes memory message,bytes32[] memory merkleProof) public",
        ])
        .unwrap(),
    );
}

pub async fn run(args: MigrateFromGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;
    let gateway_chain_id = gateway_chain_config.chain_id.as_u64();
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;

    let preparation_config_path = GATEWAY_PREPARATION.input(&ecosystem_config.link_to_code);
    let preparation_config = GatewayPreparationConfig::new(
        &gateway_chain_config,
        &gateway_chain_config.get_contracts_config()?,
        &ecosystem_config.get_contracts_config()?,
        &gateway_gateway_config,
    )?;
    preparation_config.save(shell, preparation_config_path)?;

    let chain_contracts_config = chain_config.get_contracts_config()?;
    let gateway_chain_chain_config = chain_config.get_gateway_chain_config().await?;
    let chain_admin_addr = chain_contracts_config.l1.chain_admin_addr;
    let chain_access_control_restriction =
        chain_contracts_config.l1.access_control_restriction_addr;

    println!("Migrating the chain to L1...");
    let hash = call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "startMigrateChainFromGateway",
                (
                    chain_admin_addr,
                    chain_access_control_restriction.context("chain_access_control_restriction")?,
                    gateway_chain_chain_config.chain_admin_addr()?,
                    U256::from(chain_config.chain_id.as_u64()),
                ),
            )
            .unwrap(),
        &ecosystem_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?;

    let general_config = gateway_chain_config.get_general_config().await?;
    let l2_rpc_url = general_config.l2_http_url()?;
    let gateway_provider = Provider::<Http>::try_from(&l2_rpc_url)?;

    let client: Client<L2> = Client::http(l2_rpc_url.parse().context("invalid L2 RPC URL")?)?
        .for_network(L2::from(L2ChainId::new(gateway_chain_id).unwrap()))
        .build();

    if hash == H256::zero() {
        println!("Chain already migrated!");
    } else {
        println!("Migration started! Migration hash: {}", hex::encode(hash));
        await_for_tx_to_complete(&gateway_provider, hash).await?;
        await_for_withdrawal_to_finalize(&client, hash).await?;
    }
    // FIXME: this is a temporary hack to make sure that the withdrawal is processed.
    tokio::time::sleep(tokio::time::Duration::from_millis(60000)).await;

    let params = client.get_finalize_withdrawal_params(hash, 0).await?;

    call_script(
        shell,
        args.forge_args.clone(),
        &GATEWAY_PREPARATION_INTERFACE
            .encode(
                "finishMigrateChainFromGateway",
                (
                    U256::from(chain_config.chain_id.as_u64()),
                    U256::from(gateway_chain_id),
                    U256::from(params.l2_batch_number.0[0]),
                    U256::from(params.l2_message_index.0[0]),
                    U256::from(params.l2_tx_number_in_block.0[0]),
                    params.message,
                    params.proof.proof,
                ),
            )
            .unwrap(),
        &ecosystem_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url.clone(),
    )
    .await?;

    let l1_da_validator_addr = get_l1_da_validator(&chain_config)
        .await
        .context("l1_da_validator_addr")?;

    let spinner = Spinner::new(MSG_DA_PAIR_REGISTRATION_SPINNER);
    set_da_validator_pair(
        shell,
        &ecosystem_config,
        chain_contracts_config.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        chain_contracts_config.l1.diamond_proxy_addr,
        l1_da_validator_addr,
        chain_contracts_config
            .l2
            .da_validator_addr
            .context("da_validator_addr")?,
        &args.forge_args,
        l1_url.clone(),
    )
    .await?;
    spinner.finish();
    Ok(())
}

async fn await_for_tx_to_complete(
    gateway_provider: &Provider<Http>,
    hash: H256,
) -> anyhow::Result<()> {
    println!("Waiting for transaction to complete...");
    while gateway_provider
        .get_transaction_receipt(hash)
        .await?
        .is_none()
    {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // We do not handle network errors
    let receipt = gateway_provider
        .get_transaction_receipt(hash)
        .await?
        .unwrap();

    if receipt.status == Some(U64::from(1)) {
        println!("Transaction completed successfully!");
    } else {
        panic!("Transaction failed!");
    }

    Ok(())
}

async fn await_for_withdrawal_to_finalize(
    gateway_provider: &Client<L2>,
    hash: H256,
) -> anyhow::Result<()> {
    println!("Waiting for withdrawal to finalize...");
    while gateway_provider.get_withdrawal_log(hash, 0).await.is_err() {
        println!("Waiting for withdrawal to finalize...");
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &EcosystemConfig,
    governor: &Wallet,
    rpc_url: String,
) -> anyhow::Result<H256> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let gateway_preparation_script_output =
        GatewayPreparationOutput::read(shell, GATEWAY_PREPARATION.output(&config.link_to_code))?;

    Ok(gateway_preparation_script_output.governance_l2_tx_hash)
}
