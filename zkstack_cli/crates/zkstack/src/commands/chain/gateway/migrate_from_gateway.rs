use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
    providers::{Http, Provider},
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    ethereum::{get_ethers_provider, get_zk_client},
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    wallets::Wallet,
    zks_provider::{FinalizeWithdrawalParams, ZKSProvider},
};
use zkstack_cli_config::{
    forge_interface::script_params::GATEWAY_UTILS_SCRIPT_PATH, EcosystemConfig,
};
use zksync_basic_types::{H256, U256};
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::EthNamespaceClient,
};

use crate::{
    abi::ZkChainAbi,
    admin_functions::{set_da_validator_pair, start_migrate_chain_from_gateway},
    commands::chain::{
        admin_call_builder::AdminCallBuilder,
        gateway::{
            constants::DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
            gateway_common::extract_and_wait_for_priority_ops,
        },
        init::get_l1_da_validator,
        utils::send_tx,
    },
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
    static ref GATEWAY_UTILS_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finishMigrateChainFromGateway(address bridgehubAddr, uint256 migratingChainId, uint256 gatewayChainId, uint256 l2BatchNumber, uint256 l2MessageIndex, uint16 l2TxNumberInBatch, bytes memory message, bytes32[] memory merkleProof) public",
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

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let chain_contracts_config = chain_config.get_contracts_config()?;

    let l1_diamond_cut_data = ecosystem_config
        .get_contracts_config()?
        .ecosystem_contracts
        .diamond_cut_data;

    let start_migrate_from_gateway_call = start_migrate_chain_from_gateway(
        shell,
        &args.forge_args,
        &ecosystem_config.path_to_l1_foundry(),
        crate::admin_functions::AdminScriptMode::OnlySave,
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
        chain_config.chain_id.as_u64(),
        gateway_chain_config.chain_id.as_u64(),
        hex::decode(&l1_diamond_cut_data)
            .context("Failed to decode diamond cut data")?
            .into(),
        chain_config.get_wallets_config()?.operator.address,
        l1_url.clone(),
    )
    .await?;

    let chain_admin = start_migrate_from_gateway_call.admin_address;
    let (calldata, value) =
        AdminCallBuilder::new(start_migrate_from_gateway_call.calls).compile_full_calldata();

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.l2_http_url()?;
    let gateway_provider = get_ethers_provider(&gw_rpc_url)?;
    let gateway_zk_client = get_zk_client(&gw_rpc_url, chain_config.chain_id.as_u64())?;

    if calldata.is_empty() {
        logger::info("Chain already migrated!");
        return Ok(());
    }

    logger::info("Starting the migration!");
    let receipt = send_tx(
        chain_admin,
        calldata,
        value,
        l1_url.clone(),
        chain_config
            .get_wallets_config()?
            .governor
            .private_key_h256()
            .unwrap(),
        "migrating from gateway",
    )
    .await?;

    let priority_ops = extract_and_wait_for_priority_ops(
        receipt,
        gateway_chain_config
            .get_contracts_config()?
            .l1
            .diamond_proxy_addr,
        gateway_provider,
    )
    .await?;

    assert!(
        !priority_ops.is_empty(),
        "No priority op hashes were emitted during the withdrawal calls"
    );

    let last_priority_op_hash = *priority_ops.last().unwrap();

    await_for_withdrawal_to_finalize(
        &gateway_zk_client,
        get_ethers_provider(&l1_url)?,
        gateway_chain_config
            .get_contracts_config()?
            .l1
            .diamond_proxy_addr,
        last_priority_op_hash,
    )
    .await?;

    let params = gateway_zk_client
        .get_finalize_withdrawal_params(last_priority_op_hash, 0)
        .await?;

    finish_migrate_chain_from_gateway(
        shell,
        args.forge_args.clone(),
        &ecosystem_config.path_to_l1_foundry(),
        ecosystem_config
            .get_wallets()?
            .deployer
            .context("Missing deployer wallet")?,
        ecosystem_config
            .get_contracts_config()?
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        chain_config.chain_id.as_u64(),
        gateway_chain_id,
        params,
        l1_url.clone(),
    )
    .await?;

    let l1_da_validator_addr = get_l1_da_validator(&chain_config)
        .await
        .context("l1_da_validator_addr")?;

    let spinner = Spinner::new(MSG_DA_PAIR_REGISTRATION_SPINNER);
    set_da_validator_pair(
        shell,
        &args.forge_args,
        &ecosystem_config.path_to_l1_foundry(),
        crate::admin_functions::AdminScriptMode::Broadcast(
            chain_config.get_wallets_config()?.governor,
        ),
        chain_config.chain_id.as_u64(),
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        l1_da_validator_addr,
        chain_contracts_config
            .l2
            .da_validator_addr
            .context("da_validator_addr")?,
        l1_url.clone(),
    )
    .await?;
    spinner.finish();
    Ok(())
}

const LOOK_WAITING_TIME_MS: u64 = 1600;

pub(crate) async fn check_whether_gw_transaction_is_finalized(
    gateway_provider: &Client<L2>,
    l1_provider: Arc<Provider<Http>>,
    gateway_diamond_proxy: Address,
    hash: H256,
) -> anyhow::Result<bool> {
    let Some(receipt) = gateway_provider.get_transaction_receipt(hash).await? else {
        return Ok(false);
    };

    if receipt.l1_batch_number.is_none() {
        return Ok(false);
    }

    let batch_number = receipt.l1_batch_number.unwrap();

    if gateway_provider
        .get_finalize_withdrawal_params(hash, 0)
        .await
        .is_err()
    {
        return Ok(false);
    }

    // TODO(PLA-1121): investigate why waiting for the tx proof is not enough.
    // This is not expected behavior.
    let gateway_contract = ZkChainAbi::new(gateway_diamond_proxy, l1_provider);
    Ok(gateway_contract.get_total_batches_executed().await? >= U256::from(batch_number.as_u64()))
}

async fn await_for_withdrawal_to_finalize(
    gateway_provider: &Client<L2>,
    l1_provider: Arc<Provider<Http>>,
    gateway_diamond_proxy: Address,
    hash: H256,
) -> anyhow::Result<()> {
    while !check_whether_gw_transaction_is_finalized(
        gateway_provider,
        l1_provider.clone(),
        gateway_diamond_proxy,
        hash,
    )
    .await?
    {
        println!("Waiting for withdrawal to finalize...");
        tokio::time::sleep(tokio::time::Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish_migrate_chain_from_gateway(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    foundry_scripts_path: &Path,
    wallet: Wallet,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    params: FinalizeWithdrawalParams,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let data = GATEWAY_UTILS_INTERFACE
        .encode(
            "finishMigrateChainFromGateway",
            (
                l1_bridgehub_addr,
                U256::from(l2_chain_id),
                U256::from(gateway_chain_id),
                U256::from(params.l2_batch_number.0[0]),
                U256::from(params.l2_message_index.0[0]),
                U256::from(params.l2_tx_number_in_block.0[0]),
                params.message,
                params.proof.proof,
            ),
        )
        .unwrap();

    let mut forge = Forge::new(foundry_scripts_path)
        .script(
            &PathBuf::from(GATEWAY_UTILS_SCRIPT_PATH),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Deployer)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    Ok(())
}
