use std::{num::NonZeroUsize, str::FromStr};

use anyhow::Context;
use clap::Parser;
use ethers::{
    contract::abigen,
    providers::{Http, Provider},
    utils::hex,
};
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::ethereum::{get_ethers_provider, get_zk_client};
use zkstack_cli_config::traits::{ReadConfig, ZkStackConfig};
use zksync_basic_types::{
    protocol_version::ProtocolVersionId, url::SensitiveUrl, web3::Bytes, Address, L1BatchNumber,
    L2BlockNumber, L2ChainId, U256,
};
use zksync_web3_decl::{
    client::{Client, DynClient, L2},
    namespaces::ZksNamespaceClient,
};

use crate::commands::dev::commands::upgrade_utils::{
    print_error, set_upgrade_timestamp_calldata, AdminCallBuilder,
};

#[derive(Debug, Default)]
pub struct FetchedChainInfo {
    hyperchain_addr: Address,
    chain_admin_addr: Address,
}

// Bridgehub ABI
abigen!(
    BridgehubAbi,
    r"[
    function getHyperchain(uint256)(address)
]"
);

// L1SharedBridgeLegacyStore ABI
abigen!(
    L1SharedBridgeLegacyAbi,
    r"[
    function l2BridgeAddress(uint256 _chainId)(address)
]"
);

// L2WrappedBaseTokenStore ABI
abigen!(
    L2WrappedBaseTokenStoreAbi,
    r"[
    function l2WBaseTokenAddress(uint256 _chainId)(address)
]"
);

// L2WrappedBaseTokenStore ABI
abigen!(
    L2NativeTokenVaultAbi,
    r"[
    function assetId(address)(bytes32)
    function L2_LEGACY_SHARED_BRIDGE()(address)
]"
);

abigen!(
    L2LegacySharedBridgeAbi,
    r"[
    function l1TokenAddress(address)(address)
]"
);

// ZKChain ABI
abigen!(
    ZKChainAbi,
    r"[
    function getPubdataPricingMode()(uint256)
    function getBaseToken()(address)
    function getAdmin()(address)
    function getTotalBatchesCommitted() external view returns (uint256)
    function getTotalBatchesVerified() external view returns (uint256)
]"
);

// ZKChain ABI
abigen!(
    ValidatorTimelockAbi,
    r"[
    function validators(uint256 _chainId, address _validator)(bool)
]"
);

async fn verify_next_batch_new_version(
    batch_number: u32,
    main_node_client: &DynClient<L2>,
) -> anyhow::Result<()> {
    let (_, right_bound) = main_node_client
        .get_l2_block_range(L1BatchNumber(batch_number))
        .await?
        .context("Range must be present for a batch")?;

    let next_l2_block = right_bound + 1;

    let block_details = main_node_client
        .get_block_details(L2BlockNumber(next_l2_block.as_u32()))
        .await?
        .with_context(|| format!("No L2 block is present after the batch {}", batch_number))?;

    let protocol_version = block_details.protocol_version.with_context(|| {
        format!(
            "Protocol version not present for block {}",
            next_l2_block.as_u64()
        )
    })?;
    anyhow::ensure!(
        protocol_version >= ProtocolVersionId::Version27,
        "THe block does not yet contain the v27 (EVM Interpreter) upgrade"
    );

    Ok(())
}

pub async fn check_chain_readiness(
    l1_rpc_url: String,
    l2_rpc_url: String,
    l2_chain_id: u64,
) -> anyhow::Result<()> {
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;

    let l2_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    let diamond_proxy_addr = l2_client.get_main_l1_contract().await?;

    let zkchain = ZKChainAbi::new(diamond_proxy_addr, l1_provider.clone());
    let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
    let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

    verify_next_batch_new_version(batches_committed, &l2_client).await?;
    verify_next_batch_new_version(batches_verified, &l2_client).await?;

    Ok(())
}

pub async fn fetch_chain_info(
    upgrade_info: &V27UpgradeInfo,
    args: &V27EvmInterpreterUpgradeArgsInner,
) -> anyhow::Result<FetchedChainInfo> {
    // Connect to the L1 Ethereum network
    let l1_provider = get_ethers_provider(&args.l1_rpc_url)?;
    let chain_id = U256::from(args.chain_id);

    let bridgehub = BridgehubAbi::new(upgrade_info.bridgehub_addr, l1_provider.clone());
    let hyperchain_addr = bridgehub.get_hyperchain(chain_id).await?;
    if hyperchain_addr == Address::zero() {
        anyhow::bail!("Chain not present in bridgehub");
    }

    let zkchain = ZKChainAbi::new(hyperchain_addr, l1_provider.clone());

    let chain_admin_addr = zkchain.get_admin().await?;

    Ok(FetchedChainInfo {
        hyperchain_addr,
        chain_admin_addr,
    })
}

#[derive(Debug, Clone, Serialize)]
struct AdminCall {
    description: String,
    target: Address,
    #[serde(serialize_with = "serialize_hex")]
    data: Vec<u8>,
    value: U256,
}

fn serialize_hex<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let hex_string = format!("0x{}", hex::encode(bytes));
    serializer.serialize_str(&hex_string)
}

#[derive(Parser, Debug, Clone)]
pub struct V27EvmInterpreterCalldataArgs {
    upgrade_description_path: String,
    chain_id: u64,
    l1_rpc_url: String,
    l2_rpc_url: String,
    server_upgrade_timestamp: u64,
    #[clap(long, default_missing_value = "false")]
    dangerous_no_cross_check: Option<bool>,
    #[clap(long, default_missing_value = "false")]
    force_display_finalization_params: Option<bool>,
}

pub struct V27EvmInterpreterUpgradeArgsInner {
    pub chain_id: u64,
    pub l1_rpc_url: String,
}

impl From<V27EvmInterpreterCalldataArgs> for V27EvmInterpreterUpgradeArgsInner {
    fn from(value: V27EvmInterpreterCalldataArgs) -> Self {
        Self {
            chain_id: value.chain_id,
            l1_rpc_url: value.l1_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V27UpgradeInfo {
    // Information about pre-upgrade contracts.
    l1_chain_id: u32,
    pub(crate) bridgehub_addr: Address,

    // Information from upgrade
    chain_upgrade_diamond_cut: Bytes,

    new_protocol_version: u64,
    old_protocol_version: u64,
}

impl ZkStackConfig for V27UpgradeInfo {}

pub(crate) async fn run(shell: &Shell, args: V27EvmInterpreterCalldataArgs) -> anyhow::Result<()> {
    // 0. Read the GatewayUpgradeInfo

    let upgrade_info = V27UpgradeInfo::read(shell, &args.upgrade_description_path)?;

    // 1. Update all the configs

    let chain_info = fetch_chain_info(&upgrade_info, &args.clone().into()).await?;

    // 2. Generate calldata
    let schedule_calldata = set_upgrade_timestamp_calldata(
        upgrade_info.new_protocol_version,
        args.server_upgrade_timestamp,
    );

    let set_timestamp_call = AdminCall {
        description: "Calldata to schedule upgrade".to_string(),
        data: schedule_calldata,
        target: chain_info.chain_admin_addr,
        value: U256::zero(),
    };
    println!("{}", serde_json::to_string_pretty(&set_timestamp_call)?);
    println!("---------------------------");

    if !args.force_display_finalization_params.unwrap_or_default() {
        let chain_readiness = check_chain_readiness(
            args.l1_rpc_url.clone(),
            args.l2_rpc_url.clone(),
            args.chain_id,
        )
        .await;

        if let Err(err) = chain_readiness {
            print_error(err);
            return Ok(());
        };
    }

    let mut admin_calls_finalize = AdminCallBuilder::new();

    admin_calls_finalize.append_execute_upgrade(
        chain_info.hyperchain_addr,
        upgrade_info.old_protocol_version,
        upgrade_info.chain_upgrade_diamond_cut.clone(),
    );

    admin_calls_finalize.display();

    let chain_admin_calldata = admin_calls_finalize.compile_full_calldata();

    println!(
        "Full calldata to call `ChainAdmin` with : {}",
        hex::encode(&chain_admin_calldata)
    );

    Ok(())
}
