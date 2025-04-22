use std::{path::Path, sync::Arc};

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
    gateway_migration_calldata::{get_migrate_to_gateway_calls, MigrateToGatewayParams},
    notify_server_calldata::{get_notify_server_calls, NotifyServerCalldataArgs},
    utils::{display_admin_script_output, get_default_foundry_path, get_zk_client},
};
use crate::{
    accept_ownership::{
        admin_l1_l2_tx, enable_validator_via_gateway, finalize_migrate_to_gateway,
        notify_server_migration_from_gateway, notify_server_migration_to_gateway,
        set_da_validator_pair_via_gateway, AdminScriptOutput,
    },
    commands::chain::utils::get_ethers_provider,
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

// 50 gwei
const MAX_EXPECTED_L1_GAS_PRICE: u64 = 50_000_000_000;

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
    let chain_access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("chain_access_control_restriction")?;

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
            max_l1_gas_price: MAX_EXPECTED_L1_GAS_PRICE,
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

pub(crate) async fn await_for_tx_to_complete(
    gateway_provider: &Arc<Provider<Http>>,
    hash: H256,
) -> anyhow::Result<()> {
    logger::info(&format!(
        "Waiting for transaction with hash {:#?} to complete...",
        hash
    ));
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
        logger::info("Transaction completed successfully!");
    } else {
        panic!("Transaction failed! Receipt: {:?}", receipt);
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum MigrationDirection {
    FromGateway,
    ToGateway,
}

impl MigrationDirection {
    pub(crate) fn expected_notificaation(self) -> GatewayMigrationNotification {
        match self {
            Self::FromGateway => GatewayMigrationNotification::FromGateway,
            Self::ToGateway => GatewayMigrationNotification::ToGateway,
        }
    }
}

pub(crate) async fn notify_server(
    args: ForgeScriptArgs,
    shell: &Shell,
    direction: MigrationDirection,
) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let contracts = chain_config.get_contracts_config()?;

    let calls = get_notify_server_calls(
        shell,
        &args,
        &chain_config.path_to_l1_foundry(),
        NotifyServerCalldataArgs {
            l1_bridgehub_addr: contracts.ecosystem_contracts.bridgehub_proxy_addr,
            l2_chain_id: chain_config.chain_id.as_u64(),
            l1_rpc_url: l1_url.clone(),
        },
        direction,
    )
    .await?;

    let (data, value) = AdminCallBuilder::new(calls.calls).compile_full_calldata();

    send_tx(
        calls.admin_address,
        data,
        value,
        l1_url,
        chain_config
            .get_wallets_config()?
            .governor
            .private_key_h256()
            .unwrap(),
    )
    .await?;

    Ok(())
}

pub(crate) async fn send_tx(
    to: Address,
    data: Vec<u8>,
    value: U256,
    l1_rpc_url: String,
    private_key: H256,
) -> anyhow::Result<TransactionReceipt> {
    // 1. Connect to provider
    let provider = Provider::<Http>::try_from(&l1_rpc_url)?;

    // 2. Set up wallet (signer)
    let wallet: LocalWallet = LocalWallet::from_bytes(private_key.as_bytes())?;
    let wallet = wallet.with_chain_id(provider.get_chainid().await?.as_u64()); // Mainnet

    // 3. Create a transaction
    let tx = TransactionRequest::new().to(to).data(data).value(value);

    // 4. Sign the transaction
    let client = SignerMiddleware::new(provider.clone(), wallet.clone());
    let pending_tx = client.send_transaction(tx, None).await?;

    println!(
        "Transaction {:#?} has been sent! Waiting...",
        pending_tx.tx_hash()
    );

    // 5. Await receipt
    let receipt: TransactionReceipt = pending_tx.await?.context("Receipt not found")?;

    println!("Transaciton {:#?} confirmed!", receipt.transaction_hash);

    Ok(receipt)
}

pub(crate) async fn extract_and_wait_for_priority_ops(
    receipt: TransactionReceipt,
    expected_diamond_proxy: Address,
    gateway_provider: Arc<Provider<Http>>,
) -> anyhow::Result<Vec<H256>> {
    let priority_ops = extract_priority_ops(receipt, expected_diamond_proxy).await?;

    logger::info(format!(
        "Migration has produced a total of {} priority operations for Gateway",
        priority_ops.len()
    ));
    for hash in priority_ops.iter() {
        await_for_tx_to_complete(&gateway_provider, *hash).await?;
    }

    Ok(priority_ops)
}

pub(crate) async fn extract_priority_ops(
    receipt: TransactionReceipt,
    expected_diamond_proxy: Address,
) -> anyhow::Result<Vec<H256>> {
    // TODO(EVM-749): cleanup the constant and automate its derivation
    let expected_topic_0: H256 = "4531cd5795773d7101c17bdeb9f5ab7f47d7056017506f937083be5d6e77a382"
        .parse()
        .unwrap();

    let priority_ops = receipt
        .logs
        .into_iter()
        .filter_map(|log| {
            if log.topics.is_empty() || log.topics[0] != expected_topic_0 {
                return None;
            }
            if log.address != expected_diamond_proxy {
                return None;
            }

            Some(H256::from_slice(&log.data[32..64]))
        })
        .collect();

    Ok(priority_ops)
}
