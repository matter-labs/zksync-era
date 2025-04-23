use std::{hash::Hash, path::Path, sync::Arc};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{encode, parse_abi, ParamType, Token},
    contract::{abigen, BaseContract},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Bytes, Filter, TransactionReceipt, TransactionRequest},
    utils::{hex, keccak256},
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::{
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GatewayChainConfigPatch,
};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::{Address, H256, U256, U64};
use zksync_contracts::chain_admin_contract;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_types::{
    address_to_u256, h256_to_u256, server_notification::GatewayMigrationNotification,
    u256_to_address, u256_to_h256, web3::ValueOrArray,
};
use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::{
    admin_call_builder::{self, AdminCall, AdminCallBuilder},
    gateway_common::{extract_and_wait_for_priority_ops, send_tx, MigrationDirection},
    migrate_to_gateway_calldata::{get_migrate_to_gateway_calls, MigrateToGatewayParams},
    notify_server_calldata::{get_notify_server_calls, NotifyServerCallsArgs},
    utils::{display_admin_script_output, get_default_foundry_path, get_zk_client},
};
use crate::{
    accept_ownership::{
        admin_l1_l2_tx, enable_validator_via_gateway, finalize_migrate_to_gateway,
        notify_server_migration_from_gateway, notify_server_migration_to_gateway,
        set_da_validator_pair_via_gateway, AdminScriptOutput,
    },
    commands::chain::utils::get_ethers_provider,
    consts::DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
}

abigen!(
    BridgehubAbi,
    r"[
    function settlementLayer(uint256)(uint256)
    function getZKChain(uint256)(address)
    function ctmAssetIdToAddress(bytes32)(address)
    function ctmAssetIdFromChainId(uint256)(bytes32)
    function baseTokenAssetId(uint256)(bytes32)
    function chainTypeManager(uint256)(address)
]"
);

abigen!(
    ZkChainAbi,
    r"[
    function getDAValidatorPair()(address,address)
    function getAdmin()(address)
    function getProtocolVersion()(uint256)
]"
);

abigen!(
    ChainTypeManagerAbi,
    r"[
    function validatorTimelock()(address)
    function forwardedBridgeMint(uint256 _chainId,bytes calldata _ctmData)(address)
    function serverNotifierAddress()(address)
]"
);

abigen!(
    ValidatorTimelockAbi,
    r"[
    function validators(uint256 _chainId, address _validator)(bool)
]"
);

pub async fn run(args: MigrateToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
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

    let genesis_config = chain_config.get_genesis_config().await?;
    let gateway_contract_config = gateway_chain_config.get_contracts_config()?;

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();

    logger::info("Migrating the chain to the Gateway...");

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.l2_http_url()?;

    let is_rollup = matches!(
        genesis_config.l1_batch_commitment_mode()?,
        L1BatchCommitmentMode::Rollup
    );

    let gateway_da_validator_address = if is_rollup {
        gateway_gateway_config.relayed_sl_da_validator
    } else {
        gateway_gateway_config.validium_da_validator
    };
    let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    let (chain_admin, calls) = get_migrate_to_gateway_calls(
        shell,
        &args.forge_args,
        &chain_config.path_to_l1_foundry(),
        MigrateToGatewayParams {
            l1_rpc_url: l1_url.clone(),
            l1_bridgehub_addr: chain_contracts_config
                .ecosystem_contracts
                .bridgehub_proxy_addr,
            max_l1_gas_price: DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
            l2_chain_id: chain_config.chain_id.as_u64(),
            gateway_chain_id: gateway_chain_config.chain_id.as_u64(),
            gateway_diamond_cut: gateway_gateway_config.diamond_cut_data.0.clone().into(),
            gateway_rpc_url: gw_rpc_url.clone(),
            new_sl_da_validator: gateway_da_validator_address,
            validator_1: chain_secrets_config.blob_operator.address,
            validator_2: chain_secrets_config.operator.address,
            min_validator_balance: U256::from(10).pow(19.into()).into(),
            refund_recipient: None,
        },
    )
    .await?;

    if calls.is_empty() {
        logger::info("Chain already migrated!");
        return Ok(());
    }

    let (calldata, value) = AdminCallBuilder::new(calls).compile_full_calldata();

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
        "migrating to gateway",
    )
    .await?;

    let gateway_provider = get_ethers_provider(&gw_rpc_url)?;
    extract_and_wait_for_priority_ops(
        receipt,
        gateway_contract_config.l1.diamond_proxy_addr,
        gateway_provider.clone(),
    )
    .await?;

    let mut chain_secrets_config = chain_config.get_secrets_config().await?.patched();
    chain_secrets_config.set_gateway_rpc_url(gw_rpc_url)?;
    chain_secrets_config.save().await?;

    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gateway_provider);

    let mut gateway_chain_config =
        GatewayChainConfigPatch::empty(shell, chain_config.path_to_gateway_chain_config());
    gateway_chain_config.init(
        &gateway_gateway_config,
        gw_bridgehub
            .get_zk_chain(chain_config.chain_id.as_u64().into())
            .await?,
        // FIXME: no chain admin is supported here
        Address::zero(),
        gateway_chain_id.into(),
    )?;
    gateway_chain_config.save().await?;

    Ok(())
}
