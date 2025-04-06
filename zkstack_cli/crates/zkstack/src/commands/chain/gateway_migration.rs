use std::{path::Path, sync::Arc};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{encode, parse_abi, ParamType, Token},
    contract::{abigen, BaseContract},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Bytes, TransactionReceipt, TransactionRequest},
    utils::hex,
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
    forge_interface::{
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::GATEWAY_PREPARATION,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::{Address, H256, U256, U64};
use zksync_config::configs::gateway::GatewayChainConfig;
use zksync_contracts::chain_admin_contract;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_types::{address_to_u256, u256_to_address};

use super::admin_call_builder::{
    self, encode_admin_multicall, get_ethers_provider, AdminCall, AdminCallBuilder,
};
use crate::{
    accept_ownership::{
        enable_validator_via_gateway, finalize_migrate_to_gateway,
        set_da_validator_pair_via_gateway,
    },
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

lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function migrateChainToGateway(address chainAdmin,address l2ChainAdmin,address accessControlRestriction,uint256 chainId) public",
            "function setDAValidatorPair(address accessControlRestriction,uint256 chainId,address l1DAValidator,address l2DAValidator,address chainDiamondProxyOnGateway)",
            "function supplyGatewayWallet(address addr, uint256 addr) public",
            "function enableValidator(address accessControlRestriction,uint256 chainId,address validatorAddress,address gatewayValidatorTimelock) public",
            "function grantWhitelist(address filtererProxy, address[] memory addr) public",
            "function deployL2ChainAdmin() public",
            "function notifyServerMigrationFromGateway(address serverNotifier, address chainAdmin, address accessControlRestriction, uint256 chainId) public",
            "function notifyServerMigrationToGateway(address serverNotifier, address chainAdmin, address accessControlRestriction, uint256 chainId) public"
        ])
        .unwrap(),
    );

    static ref BRIDGEHUB_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function getHyperchain(uint256 chainId) public returns (address)"
        ])
        .unwrap(),
    );
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
]"
);

abigen!(
    ValidatorTimelockAbi,
    r"[
    function validators(uint256 _chainId, address _validator)(bool)
]"
);

const IS_PERMANENT_ROLLUP_SLOT: u64 = 57;

fn apply_l1_to_l2_alias(addr: Address) -> Address {
    let offset: Address = "1111000000000000000000000000000000001111".parse().unwrap();
    let addr_with_offset = address_to_u256(&addr) + address_to_u256(&offset);

    u256_to_address(&addr_with_offset)
}

// The most reliable way to precompute the address is to simulate `createNewChain` function
async fn precompute_chain_address_on_gateway(
    l2_chain_id: u64,
    base_token_asset_id: H256,
    new_l2_admin: Address,
    protocol_version: U256,
    gateway_diamond_cut: Vec<u8>,
    gw_ctm: ChainTypeManagerAbi<Provider<ethers::providers::Http>>,
) -> anyhow::Result<Address> {
    let ctm_data = encode(&[
        Token::FixedBytes(base_token_asset_id.0.into()),
        Token::Address(new_l2_admin),
        Token::Uint(protocol_version),
        Token::Bytes(gateway_diamond_cut),
    ]);

    let result = gw_ctm
        .forwarded_bridge_mint(l2_chain_id.into(), ctm_data.into())
        .from(L2_BRIDGEHUB_ADDRESS)
        .await?;

    Ok(result)
}

pub(crate) async fn get_migrate_to_gateway_calls(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    max_l1_gas_price: u64,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    gateway_diamond_cut: Vec<u8>,
    gateway_rpc_url: String,
    new_sl_da_validator: Address,
    validator_1: Address,
    validator_2: Address,
    refund_recipient: Option<Address>,
) -> anyhow::Result<(Address, Vec<AdminCall>)> {
    // TODO: add checks about chain notification.

    // TODO: use refund recipient in scripts for L1->L2 communication
    let refund_recipient = refund_recipient.unwrap_or(validator_1);
    let mut result = vec![];

    let l1_provider = get_ethers_provider(&l1_rpc_url)?;
    let gw_provider = get_ethers_provider(&gateway_rpc_url)?;

    let l1_bridgehub = BridgehubAbi::new(l1_bridgehub_addr, l1_provider.clone());
    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_provider.clone());

    let current_settlement_layer = l1_bridgehub.settlement_layer(l2_chain_id.into()).await?;

    let zk_chain_l1_address = l1_bridgehub.get_zk_chain(l2_chain_id.into()).await?;

    if zk_chain_l1_address == Address::zero() {
        anyhow::bail!("Chain with id {} does not exist!", l2_chain_id);
    }

    // Checking whether the user has already done the migration
    if current_settlement_layer == U256::from(gateway_chain_id) {
        // TODO: it may happen that the user has started the migration, but it failed for some reason (e.g. the provided
        // diamond cut was not correct).
        // The recovery of the chain is not handled by the tool right now.
        anyhow::bail!("The chain is already on top of Gateway!");
    }

    let ctm_asset_id = l1_bridgehub
        .ctm_asset_id_from_chain_id(l2_chain_id.into())
        .await?;
    let ctm_gw_address = gw_bridgehub.ctm_asset_id_to_address(ctm_asset_id).await?;

    if ctm_gw_address == Address::zero() {
        anyhow::bail!("{gateway_chain_id} does not have a CTM deployed!");
    }

    let gw_ctm = ChainTypeManagerAbi::new(ctm_gw_address, gw_provider.clone());
    let gw_validator_timelock_addr = gw_ctm.validator_timelock().await?;
    let gw_validator_timelock =
        ValidatorTimelockAbi::new(gw_validator_timelock_addr, gw_provider.clone());

    let l1_zk_chain = ZkChainAbi::new(zk_chain_l1_address, l1_provider.clone());
    let chain_admin_address = l1_zk_chain.get_admin().await?;
    let zk_chain_gw_address = {
        let recorded_zk_chain_gw_address = gw_bridgehub.get_zk_chain(l2_chain_id.into()).await?;
        if recorded_zk_chain_gw_address == Address::zero() {
            let expected_address = precompute_chain_address_on_gateway(
                l2_chain_id,
                H256(l1_bridgehub.base_token_asset_id(l2_chain_id.into()).await?),
                apply_l1_to_l2_alias(l1_zk_chain.get_admin().await?),
                l1_zk_chain.get_protocol_version().await?,
                gateway_diamond_cut.clone(),
                gw_ctm,
            )
            .await?;

            expected_address
        } else {
            recorded_zk_chain_gw_address
        }
    };

    let finalize_migrate_to_gateway_output = finalize_migrate_to_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::accept_ownership::AdminScriptMode::OnlySave,
        l1_bridgehub_addr,
        max_l1_gas_price,
        l2_chain_id,
        gateway_chain_id,
        gateway_diamond_cut.into(),
        l1_rpc_url.clone(),
    )
    .await?;

    result.extend(finalize_migrate_to_gateway_output.calls);

    // Changing L2 DA validator while migrating to gateway is not recommended, we allow changing only the SL one
    let (_, l2_da_validator) = l1_zk_chain.get_da_validator_pair().await?;

    // Unfortunately, there is no getter for whether a chain is a permanent rollup, we have to
    // read storage here.
    let is_permanent_rollup_slot = l1_provider
        .get_storage_at(zk_chain_l1_address, H256::from_low_u64_be(57), None)
        .await?;
    if is_permanent_rollup_slot == H256::from_low_u64_be(1) {
        // TODO: We should really check it on our own here, but it is hard with the current interfaces
        println!("WARNING: Your chain is a permanent rollup! Ensure that the new L1 SL provider is compatible with Gateway RollupDAManager!");
    }

    let da_validator_encoding_result = set_da_validator_pair_via_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::accept_ownership::AdminScriptMode::OnlySave,
        l1_bridgehub_addr,
        max_l1_gas_price.into(),
        l2_chain_id,
        gateway_chain_id,
        new_sl_da_validator,
        l2_da_validator,
        zk_chain_gw_address,
        l1_rpc_url.clone(),
    )
    .await?;

    result.extend(da_validator_encoding_result.calls);

    // 4. If validators are not yet present, please include.
    for validator in [validator_1, validator_2] {
        if !gw_validator_timelock
            .validators(l2_chain_id.into(), validator)
            .await?
        {
            let enable_validator_calls = enable_validator_via_gateway(
                shell,
                forge_args,
                foundry_contracts_path,
                crate::accept_ownership::AdminScriptMode::OnlySave,
                l1_bridgehub_addr,
                max_l1_gas_price.into(),
                l2_chain_id,
                gateway_chain_id,
                validator,
                gw_validator_timelock_addr,
                l1_rpc_url.clone(),
            )
            .await?;
            result.extend(enable_validator_calls.calls);
        }
    }

    Ok((chain_admin_address, result))
}

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

    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;

    let genesis_config = chain_config.get_genesis_config().await?;
    let gateway_contract_config = gateway_chain_config.get_contracts_config()?;

    let preparation_config_path = GATEWAY_PREPARATION.input(&ecosystem_config.link_to_code);
    let preparation_config = GatewayPreparationConfig::new(
        &gateway_chain_config,
        &gateway_contract_config,
        &ecosystem_config.get_contracts_config()?,
    )?;
    preparation_config.save(shell, preparation_config_path)?;

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();
    let chain_access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("chain_access_control_restriction")?;

    logger::info("Migrating the chain to the Gateway...");

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.get::<String>("api.web3_json_rpc.http_url")?;

    let is_rollup = matches!(
        genesis_config.get("l1_batch_commit_data_generator_mode")?,
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
        l1_url.clone(),
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        MAX_EXPECTED_L1_GAS_PRICE,
        chain_config.chain_id.as_u64(),
        gateway_chain_config.chain_id.as_u64(),
        gateway_gateway_config.diamond_cut_data.0.clone().into(),
        gw_rpc_url.clone(),
        gateway_da_validator_address,
        chain_secrets_config.blob_operator.address,
        chain_secrets_config.operator.address,
        None,
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

    let priority_ops =
        extract_priority_ops(receipt, gateway_contract_config.l1.diamond_proxy_addr).await?;

    println!(
        "Migration has produced a total of {} priority operations for Gateway",
        priority_ops.len()
    );
    let gateway_provider = get_ethers_provider(&gw_rpc_url)?;
    for hash in priority_ops {
        await_for_tx_to_complete(&gateway_provider, hash).await?;
    }

    let mut chain_secrets_config = chain_config.get_secrets_config().await?.patched();
    chain_secrets_config.insert("l1.gateway_rpc_url", gw_rpc_url)?;
    chain_secrets_config.save().await?;

    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gateway_provider);

    let gateway_chain_config = GatewayChainConfig::from_gateway_and_chain_data(
        &gateway_gateway_config,
        gw_bridgehub
            .get_zk_chain(chain_config.chain_id.as_u64().into())
            .await?,
        // FIXME: no chain admin is supported here
        Address::zero(),
        gateway_chain_id.into(),
    );
    gateway_chain_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

async fn await_for_tx_to_complete(
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

pub(crate) enum MigrationDirection {
    FromGateway,
    ToGateway,
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

    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;
    let contracts = chain_config.get_contracts_config()?;
    let server_notifier = contracts
        .ecosystem_contracts
        .server_notifier_proxy_addr
        .unwrap();
    let chain_admin = contracts.l1.chain_admin_addr;
    let restrictions = contracts
        .l1
        .access_control_restriction_addr
        .unwrap_or_default();

    let data = match direction {
        MigrationDirection::FromGateway => &GATEWAY_PREPARATION_INTERFACE.encode(
            "notifyServerMigrationFromGateway",
            (
                server_notifier,
                chain_admin,
                restrictions,
                chain_config.chain_id.as_u64(),
            ),
        )?,
        MigrationDirection::ToGateway => &GATEWAY_PREPARATION_INTERFACE.encode(
            "notifyServerMigrationToGateway",
            (
                server_notifier,
                chain_admin,
                restrictions,
                chain_config.chain_id.as_u64(),
            ),
        )?,
    };

    call_script(
        shell,
        args,
        data,
        &chain_config,
        &chain_config.get_wallets_config()?.governor,
        l1_url,
    )
    .await?;
    Ok(())
}

async fn call_script(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    data: &Bytes,
    config: &ChainConfig,
    governor: &Wallet,
    l1_rpc_url: String,
) -> anyhow::Result<GatewayPreparationOutput> {
    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(data);

    // Governor private key is required for this script
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let gateway_preparation_script_output =
        GatewayPreparationOutput::read(shell, GATEWAY_PREPARATION.output(&config.link_to_code))?;

    Ok(gateway_preparation_script_output)
}

// Default gas limit to be used for local scripts, so the exact value does not matter.
const L1_GAS_LIMIT: u64 = 2_000_000;

async fn send_tx(
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

async fn extract_priority_ops(
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
