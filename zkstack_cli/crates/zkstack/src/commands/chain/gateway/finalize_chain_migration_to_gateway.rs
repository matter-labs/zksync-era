use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
    providers::{Http, Middleware, Provider},
    types::Bytes,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    ethereum::{get_ethers_provider, get_zk_client},
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
    zks_provider::{FinalizeMigrationParams, ZKSProvider},
};
use zkstack_cli_config::{
    forge_interface::script_params::GATEWAY_UTILS_SCRIPT_PATH, ChainConfig, EcosystemConfig,
    ZkStackConfig, ZkStackConfigTrait,
};
use zksync_basic_types::{commitment::L2DACommitmentScheme, H256, U256};
use zksync_web3_decl::client::{Client, L2};

use super::migrate_to_gateway::get_migrate_to_gateway_context;
use crate::{
    admin_functions::AdminScriptMode,
    commands::chain::{
        gateway::{
            gateway_common::{extract_and_wait_for_priority_ops, get_migration_transaction},
            migrate_from_gateway::{
                check_whether_gw_transaction_is_finalized, GatewayTransactionType,
            },
            migrate_to_gateway_calldata::check_permanent_rollup_and_set_da_validator_via_gateway,
        },
        init::send_priority_txs,
    },
    messages::{
        msg_initializing_chain, MSG_CHAIN_INITIALIZED, MSG_CHAIN_NOT_INITIALIZED,
        MSG_DEPLOY_PAYMASTER_PROMPT, MSG_SELECTED_CONFIG,
    },
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct FinalizeChainMigrationToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_paymaster: Option<bool>,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub tx_status: Option<bool>,
}

lazy_static! {
    static ref GATEWAY_UTILS_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finishMigrateChainToGateway(address bridgehubAddr, bytes memory gatewayDiamondCutData, uint256 migratingChainId, uint256 gatewayChainId, bytes32 l2TxHash, uint256 l2BatchNumber, uint256 l2MessageIndex, uint16 l2TxNumberInBatch, bytes32[] memory merkleProof, uint8 txStatus) public",
        ])
        .unwrap(),
    );
}

impl FinalizeChainMigrationToGatewayArgs {
    pub fn fill_values_with_prompt(self) -> FinalizeChainMigrationToGatewayArgsFinal {
        let deploy_paymaster = self.deploy_paymaster.unwrap_or_else(|| {
            zkstack_cli_common::PromptConfirm::new(MSG_DEPLOY_PAYMASTER_PROMPT)
                .default(true)
                .ask()
        });

        FinalizeChainMigrationToGatewayArgsFinal {
            forge_args: self.forge_args,
            gateway_chain_name: self.gateway_chain_name,
            deploy_paymaster,
            tx_status: self.tx_status.unwrap_or(true),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinalizeChainMigrationToGatewayArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub gateway_chain_name: String,
    pub deploy_paymaster: bool,
    pub tx_status: bool,
}

pub async fn run(args: FinalizeChainMigrationToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;

    let args = args.fill_values_with_prompt();

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));
    logger::info(msg_initializing_chain(""));

    run_inner(
        &args,
        shell,
        &ecosystem_config,
        &chain_config,
        &gateway_chain_config,
    )
    .await?;

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}

pub async fn run_inner(
    args: &FinalizeChainMigrationToGatewayArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    gateway_chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    let l1_rpc_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;

    let general_config = gateway_chain_config.get_general_config().await?;
    let gateway_chain_id = gateway_chain_config.chain_id.as_u64();
    let gw_rpc_url = general_config.l2_http_url()?;
    let gateway_provider = get_ethers_provider(&gw_rpc_url)?;
    let gateway_zk_client = get_zk_client(&gw_rpc_url, chain_config.chain_id.as_u64())?;

    let mut contracts_config = chain_config.get_contracts_config()?;

    let gw_diamond_proxy = gateway_chain_config
        .get_contracts_config()?
        .l1
        .diamond_proxy_addr;

    let chain_migration_tx_hash = get_migration_transaction(
        &l1_rpc_url,
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        chain_config.chain_id.as_u64(),
    )
    .await?
    .context("Failed to find the transaction where the migration from L1 to GW happened")?;
    let migration_tx_receipt = l1_provider
        .get_transaction_receipt(chain_migration_tx_hash)
        .await?
        .context("Chain migration receipt not found")?;

    let priority_ops = extract_and_wait_for_priority_ops(
        migration_tx_receipt,
        gw_diamond_proxy,
        gateway_provider.clone(),
    )
    .await?;

    assert!(
        !priority_ops.is_empty(),
        "No priority op hashes were emitted during the migration calls"
    );
    let first_priority_op_hash = *priority_ops.first().unwrap();

    await_for_migration_to_finalize(
        &gateway_zk_client,
        get_ethers_provider(&l1_rpc_url)?,
        gw_diamond_proxy,
        first_priority_op_hash,
    )
    .await?;

    let params = gateway_zk_client
        .get_finalize_migration_params(first_priority_op_hash, 0)
        .await?;

    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;
    let gateway_diamond_cut = gateway_gateway_config.diamond_cut_data.0.clone();
    finish_migrate_chain_to_gateway(
        shell,
        args.forge_args.clone(),
        &chain_config.path_to_foundry_scripts(),
        ecosystem_config
            .get_wallets()?
            .deployer
            .context("Missing deployer wallet")?,
        ecosystem_config
            .get_contracts_config()?
            .core_ecosystem_contracts
            .bridgehub_proxy_addr,
        gateway_diamond_cut.into(),
        chain_config.chain_id.as_u64(),
        gateway_chain_id,
        first_priority_op_hash,
        params,
        args.tx_status,
        l1_rpc_url.clone(),
    )
    .await?;

    // Sends the priority txs that were skipped when the chain was initialized
    send_priority_txs(
        shell,
        chain_config,
        ecosystem_config,
        &mut contracts_config,
        &args.forge_args,
        l1_rpc_url,
        args.deploy_paymaster,
    )
    .await?;

    // Set the DA validator pair on the Gateway
    let context = get_migrate_to_gateway_context(chain_config, gateway_chain_config, true).await?;

    let (_, l2_da_validator_commitment_scheme) =
        context.l1_zk_chain.get_da_validator_pair().await?;
    let l2_da_validator_commitment_scheme =
        L2DACommitmentScheme::try_from(l2_da_validator_commitment_scheme)
            .map_err(|err| anyhow::format_err!("Failed to parse L2 DA commitment schema: {err}"))?;
    check_permanent_rollup_and_set_da_validator_via_gateway(
        shell,
        &args.forge_args,
        &chain_config.path_to_foundry_scripts(),
        &context,
        l2_da_validator_commitment_scheme,
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor),
    )
    .await?;

    Ok(())
}

const LOOK_WAITING_TIME_MS: u64 = 1600;

async fn await_for_migration_to_finalize(
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
        GatewayTransactionType::Migration,
    )
    .await?
    {
        println!("Waiting for migration to finalize...");
        tokio::time::sleep(tokio::time::Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish_migrate_chain_to_gateway(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    foundry_scripts_path: &Path,
    wallet: Wallet,
    l1_bridgehub_addr: Address,
    gateway_diamond_cut_data: Bytes,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    l2_tx_hash: H256,
    params: FinalizeMigrationParams,
    tx_status: bool,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    let data = GATEWAY_UTILS_INTERFACE
        .encode(
            "finishMigrateChainToGateway",
            (
                l1_bridgehub_addr,
                gateway_diamond_cut_data,
                U256::from(l2_chain_id),
                U256::from(gateway_chain_id),
                l2_tx_hash,
                U256::from(params.l2_batch_number.0[0]),
                U256::from(params.l2_message_index.0[0]),
                U256::from(params.l2_tx_number_in_block.0[0]),
                params.proof.proof,
                tx_status as u8,
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
