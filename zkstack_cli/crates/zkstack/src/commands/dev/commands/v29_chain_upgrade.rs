use anyhow::{bail, ensure, Context};
use clap::Parser;
use ethers::utils::hex;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::{get_ethers_provider, get_zk_client},
    forge::{Forge, ForgeScript, ForgeScriptArgs},
};
use zkstack_cli_config::{
    traits::{ReadConfig, ZkStackConfig},
    EcosystemConfig,
};
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
    admin_functions::{accept_admin, governance_execute_calls, set_da_validator_pair},
    commands::{
        chain::{
            admin_call_builder::{AdminCall, AdminCallBuilder},
            utils::{get_default_foundry_path, send_tx},
        },
        dev::commands::{
            upgrade_utils::{print_error, set_upgrade_timestamp_calldata},
            v29_chain_args::{V29ChainUpgradeArgs, V29UpgradeArgsInner},
        },
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
        protocol_version >= ProtocolVersionId::Version29,
        "THe block does not yet contain the v29 () upgrade"
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
    upgrade_info: &V29UpgradeInfo,
    args: &V29UpgradeArgsInner,
) -> anyhow::Result<FetchedChainInfo> {
    // Connect to the L1 Ethereum network
    let l1_provider = get_ethers_provider(&args.l1_rpc_url)?;
    let chain_id = U256::from(args.chain_id);

    let bridgehub = BridgehubAbi::new(
        upgrade_info
            .deployed_addresses
            .bridgehub
            .bridgehub_proxy_addr,
        l1_provider.clone(),
    );
    let zkchain_addr = bridgehub.get_zk_chain(chain_id).await?;
    if zkchain_addr == Address::zero() {
        bail!("Chain not present in bridgehub");
    }

    let settlement_layer = bridgehub.settlement_layer(chain_id).await?;
    let zkchain = ZkChainAbi::new(zkchain_addr, l1_provider.clone());

    let chain_admin_addr = zkchain.get_admin().await?;
    let l1_asset_router_proxy = bridgehub.asset_router().await?;

    // Repeat for GW

    // let gw_client = get_ethers_provider(&args.gw_rpc_url)?;

    // let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_client.clone());
    // let gw_zkchain_addr = gw_bridgehub.get_zk_chain(chain_id).await?;

    // if gw_zkchain_addr != Address::zero() {
    //     let gw_zkchain = ZkChainAbi::new(gw_zkchain_addr, gw_client.clone());
    //     let gz_zkchain_admin = gw_zkchain.get_admin().await?;
    //     if gz_zkchain_admin != apply_l1_to_l2_alias(chain_admin_addr) {
    //         bail!(
    //             "Provided gw_zkchain_addr ({:?}) does not match the expected aliased L1 chain_admin_addr ({:?})",
    //             gz_zkchain_admin,
    //             apply_l1_to_l2_alias(chain_admin_addr)
    //         );
    //     }
    // }

    Ok(FetchedChainInfo {
        hyperchain_addr: zkchain_addr,
        chain_admin_addr,
        gw_hyperchain_addr: Address::zero(),
        l1_asset_router_proxy,
        settlement_layer: settlement_layer.as_u64(),
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct V29UpgradeInfo {
    // Information about pre-upgrade contracts.
    l1_chain_id: u32,
    gateway_chain_id: u32,
    pub(crate) deployed_addresses: DeployedAddresses,
    pub(crate) contracts_config: ContractsConfig,

    // Information from upgrade
    chain_upgrade_diamond_cut: Bytes,
    gateway_diamond_cut: Bytes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractsConfig {
    pub(crate) new_protocol_version: u64,
    pub(crate) old_protocol_version: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployedAddresses {
    pub(crate) bridgehub: BridgehubAddresses,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgehubAddresses {
    pub(crate) bridgehub_proxy_addr: Address,
}

impl ZkStackConfig for V29UpgradeInfo {}

pub(crate) async fn run(
    shell: &Shell,
    args_input: V29ChainUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    let forge_args = &Default::default();
    let foundry_contracts_path = get_default_foundry_path(shell)?;
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let mut args = args_input.clone().fill_if_empyty(shell)?;
    if args.upgrade_description_path.is_none() {
        args.upgrade_description_path = Some(
            ecosystem_config
                .link_to_code
                .join("./contracts/l1-contracts/script-out/v29-local-output.yaml")
                .to_string_lossy()
                .to_string(),
        );
    }
    println!("args: {:?}", args);
    // 0. Read the GatewayUpgradeInfo

    let upgrade_info = V29UpgradeInfo::read(
        shell,
        &args
            .clone()
            .upgrade_description_path
            .expect("upgrade_description_path is required"),
    )?;
    println!("upgrade_info: ");
    // 1. Update all the configs

    let chain_info = fetch_chain_info(&upgrade_info, &args.clone().into()).await?;
    println!("chain_info: {:?}", chain_info);
    // 2. Generate calldata
    let schedule_calldata = set_upgrade_timestamp_calldata(
        upgrade_info.contracts_config.new_protocol_version,
        args.server_upgrade_timestamp
            .expect("server_upgrade_timestamp is required"),
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
            args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            args.l2_rpc_url.clone().expect("l2_rpc_url is required"),
            args.gw_rpc_url.clone().expect("gw_rpc_url is required"),
            args.chain_id.expect("chain_id is required"),
            args.gw_chain_id.expect("gw_chain_id is required"),
            chain_info.settlement_layer,
        )
        .await;

        if let Err(err) = chain_readiness {
            print_error(err);
            // return Ok(());
        };
    }

    let mut calldata;
    if chain_info.settlement_layer == args.gw_chain_id.unwrap() {
        let mut admin_calls_gw = AdminCallBuilder::new(vec![]);

        admin_calls_gw.append_execute_upgrade(
            chain_info.hyperchain_addr,
            upgrade_info.contracts_config.old_protocol_version,
            upgrade_info.chain_upgrade_diamond_cut.clone(),
        );

        admin_calls_gw
            .prepare_upgrade_chain_on_gateway_calls(
                shell,
                forge_args,
                &foundry_contracts_path,
                args.chain_id.expect("chain_id is required"),
                args.gw_chain_id.expect("gw_chain_id is required"),
                upgrade_info
                    .deployed_addresses
                    .bridgehub
                    .bridgehub_proxy_addr,
                args.l1_gas_price.expect("l1_gas_price is required"),
                upgrade_info.contracts_config.old_protocol_version,
                chain_info.gw_hyperchain_addr,
                chain_info.l1_asset_router_proxy,
                chain_info.chain_admin_addr,
                upgrade_info.gateway_diamond_cut.0.into(),
                args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            )
            .await;

        admin_calls_gw.display();

        let (gw_chain_admin_calldata, _) = admin_calls_gw.compile_full_calldata();
        calldata = gw_chain_admin_calldata.clone();

        println!(
            "Full calldata to call `ChainAdmin` with : {}",
            hex::encode(&gw_chain_admin_calldata)
        );
    } else {
        let mut admin_calls_finalize = AdminCallBuilder::new(vec![]);

        admin_calls_finalize.append_execute_upgrade(
            chain_info.hyperchain_addr,
            upgrade_info.contracts_config.old_protocol_version,
            upgrade_info.chain_upgrade_diamond_cut.clone(),
        );

        admin_calls_finalize.display();

        let (chain_admin_calldata, _) = admin_calls_finalize.compile_full_calldata();
        calldata = chain_admin_calldata.clone();

        println!(
            "Full calldata to call `ChainAdmin` with : {}",
            hex::encode(&chain_admin_calldata)
        );
    }

    if run_upgrade {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config.load_chain(Some("era".to_string())).context("Chain not found")?;
        // let forge_args = ForgeScriptArgs::default();
        println!("Running upgrade");

        // logger::info("Starting the migration!");
        let receipt = send_tx(
            chain_info.chain_admin_addr,
            calldata,
            U256::from(0),
            args.l1_rpc_url.clone().unwrap(),
            chain_config
                .get_wallets_config()?
                .governor
                .private_key_h256()
                .unwrap(),
            "migrating from gateway",
        )
        .await?;
        // governance_execute_calls(
        //     shell,
        //     &ecosystem_config,
        //     &ecosystem_config.get_wallets()?.governor,
        //     calldata,
        //     &forge_args,
        //     args.l1_rpc_url.clone().unwrap(),
        // )
        // .await?;
    }

    Ok(())
}
