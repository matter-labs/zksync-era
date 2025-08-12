use anyhow::{bail, ensure, Context};
use clap::Parser;
use ethers::utils::hex;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::ethereum::{get_ethers_provider, get_zk_client};
use zkstack_cli_config::traits::{ReadConfig, ZkStackConfig};
use zksync_basic_types::{
    protocol_version::ProtocolVersionId, web3::Bytes, Address, L1BatchNumber, L2BlockNumber, U256,
};
use zksync_types::L2_BRIDGEHUB_ADDRESS;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::ZksNamespaceClient,
};

use crate::{
    abi::{BridgehubAbi, ZkChainAbi},
    commands::{
        chain::{
            admin_call_builder::{AdminCall, AdminCallBuilder},
            utils::get_default_foundry_path,
        },
        dev::commands::upgrades::utils::{print_error, set_upgrade_timestamp_calldata},
    },
    utils::addresses::apply_l1_to_l2_alias,
};

#[derive(Debug, Default)]
pub struct FetchedChainInfo {
    hyperchain_addr: Address,
    chain_admin_addr: Address,
    gw_hyperchain_addr: Address,
    l1_asset_router_proxy: Address,
    settlement_layer: u64,
}

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
    ensure!(
        protocol_version >= ProtocolVersionId::Version28,
        "THe block does not yet contain the v28 (Precompiles) upgrade"
    );

    Ok(())
}

pub async fn check_chain_readiness(
    l1_rpc_url: String,
    l2_rpc_url: String,
    gw_rpc_url: String,
    l2_chain_id: u64,
    gw_chain_id: u64,
    settlement_layer: u64,
) -> anyhow::Result<()> {
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;

    let l2_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    let gw_client = get_ethers_provider(&gw_rpc_url)?;

    if settlement_layer == gw_chain_id {
        // GW
        let diamond_proxy_addr = (BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_client.clone()))
            .get_zk_chain(l2_chain_id.into())
            .await?;
        let zkchain = ZkChainAbi::new(diamond_proxy_addr, gw_client.clone());
        let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
        let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

        verify_next_batch_new_version(batches_committed, &l2_client).await?;
        verify_next_batch_new_version(batches_verified, &l2_client).await?;
    } else {
        // L1
        let diamond_proxy_addr = l2_client.get_main_l1_contract().await?;

        let zkchain = ZkChainAbi::new(diamond_proxy_addr, l1_provider.clone());
        let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
        let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

        verify_next_batch_new_version(batches_committed, &l2_client).await?;
        verify_next_batch_new_version(batches_verified, &l2_client).await?;
    }

    Ok(())
}

pub async fn fetch_chain_info(
    upgrade_info: &V28UpgradeInfo,
    args: &V28PrecompilesUpgradeArgsInner,
) -> anyhow::Result<FetchedChainInfo> {
    // Connect to the L1 Ethereum network
    let l1_provider = get_ethers_provider(&args.l1_rpc_url)?;
    let chain_id = U256::from(args.chain_id);

    let bridgehub = BridgehubAbi::new(upgrade_info.bridgehub_addr, l1_provider.clone());
    let zkchain_addr = bridgehub.get_zk_chain(chain_id).await?;
    if zkchain_addr == Address::zero() {
        bail!("Chain not present in bridgehub");
    }

    let settlement_layer = bridgehub.settlement_layer(chain_id).await?;
    let zkchain = ZkChainAbi::new(zkchain_addr, l1_provider.clone());

    let chain_admin_addr = zkchain.get_admin().await?;
    let l1_asset_router_proxy = bridgehub.asset_router().await?;

    // Repeat for GW

    let gw_client = get_ethers_provider(&args.gw_rpc_url)?;

    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_client.clone());
    let gw_zkchain_addr = gw_bridgehub.get_zk_chain(chain_id).await?;

    if gw_zkchain_addr != Address::zero() {
        let gw_zkchain = ZkChainAbi::new(gw_zkchain_addr, gw_client.clone());
        let gz_zkchain_admin = gw_zkchain.get_admin().await?;
        if gz_zkchain_admin != apply_l1_to_l2_alias(chain_admin_addr) {
            bail!(
                "Provided gw_zkchain_addr ({:?}) does not match the expected aliased L1 chain_admin_addr ({:?})",
                gz_zkchain_admin,
                apply_l1_to_l2_alias(chain_admin_addr)
            );
        }
    }

    Ok(FetchedChainInfo {
        hyperchain_addr: zkchain_addr,
        chain_admin_addr,
        gw_hyperchain_addr: gw_zkchain_addr,
        l1_asset_router_proxy,
        settlement_layer: settlement_layer.as_u64(),
    })
}

#[derive(Parser, Debug, Clone)]
pub struct V28PrecompilesCalldataArgs {
    upgrade_description_path: String,
    chain_id: u64,
    gw_chain_id: u64,
    l1_gas_price: u64,
    l1_rpc_url: String,
    l2_rpc_url: String,
    gw_rpc_url: String,
    server_upgrade_timestamp: u64,
    #[clap(long, default_missing_value = "false")]
    dangerous_no_cross_check: Option<bool>,
    #[clap(long, default_missing_value = "false")]
    force_display_finalization_params: Option<bool>,
}

pub struct V28PrecompilesUpgradeArgsInner {
    pub chain_id: u64,
    pub l1_rpc_url: String,
    pub gw_rpc_url: String,
}

impl From<V28PrecompilesCalldataArgs> for V28PrecompilesUpgradeArgsInner {
    fn from(value: V28PrecompilesCalldataArgs) -> Self {
        Self {
            chain_id: value.chain_id,
            l1_rpc_url: value.l1_rpc_url,
            gw_rpc_url: value.gw_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V28UpgradeInfo {
    // Information about pre-upgrade contracts.
    l1_chain_id: u32,
    gw_chain_id: u32,
    pub(crate) bridgehub_addr: Address,

    // Information from upgrade
    chain_upgrade_diamond_cut: Bytes,

    new_protocol_version: u64,
    old_protocol_version: u64,

    gateway_upgrade_diamond_cut: Bytes,
}

impl ZkStackConfig for V28UpgradeInfo {}

pub(crate) async fn run(shell: &Shell, args: V28PrecompilesCalldataArgs) -> anyhow::Result<()> {
    let forge_args = &Default::default();
    let foundry_contracts_path = get_default_foundry_path(shell)?;

    // 0. Read the GatewayUpgradeInfo

    let upgrade_info = V28UpgradeInfo::read(shell, &args.upgrade_description_path)?;

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
            args.gw_rpc_url.clone(),
            args.chain_id,
            args.gw_chain_id,
            chain_info.settlement_layer,
        )
        .await;

        if let Err(err) = chain_readiness {
            print_error(err);
            return Ok(());
        };
    }

    if chain_info.settlement_layer == args.gw_chain_id {
        let mut admin_calls_gw = AdminCallBuilder::new(vec![]);

        admin_calls_gw.append_execute_upgrade(
            chain_info.hyperchain_addr,
            upgrade_info.old_protocol_version,
            upgrade_info.chain_upgrade_diamond_cut.clone(),
        );

        admin_calls_gw
            .prepare_upgrade_chain_on_gateway_calls(
                shell,
                forge_args,
                &foundry_contracts_path,
                args.chain_id,
                args.gw_chain_id,
                upgrade_info.bridgehub_addr,
                args.l1_gas_price,
                upgrade_info.old_protocol_version,
                chain_info.gw_hyperchain_addr,
                chain_info.l1_asset_router_proxy,
                chain_info.chain_admin_addr,
                upgrade_info.gateway_upgrade_diamond_cut.0.into(),
                args.l1_rpc_url.clone(),
            )
            .await;

        admin_calls_gw.display();

        let (gw_chain_admin_calldata, _) = admin_calls_gw.compile_full_calldata();

        println!(
            "Full calldata to call `ChainAdmin` with : {}",
            hex::encode(&gw_chain_admin_calldata)
        );
    } else {
        let mut admin_calls_finalize = AdminCallBuilder::new(vec![]);

        admin_calls_finalize.append_execute_upgrade(
            chain_info.hyperchain_addr,
            upgrade_info.old_protocol_version,
            upgrade_info.chain_upgrade_diamond_cut.clone(),
        );

        admin_calls_finalize.display();

        let (chain_admin_calldata, _) = admin_calls_finalize.compile_full_calldata();

        println!(
            "Full calldata to call `ChainAdmin` with : {}",
            hex::encode(&chain_admin_calldata)
        );
    }

    Ok(())
}
