use anyhow::{bail, ensure, Context};
use ethers::{providers::Middleware, utils::hex};
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::{get_ethers_provider, get_zk_client},
    logger,
};
use zkstack_cli_config::{
    traits::{FileConfigTrait, ReadConfig},
    ZkStackConfig, ZkStackConfigTrait,
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
    commands::{
        chain::{
            admin_call_builder::{AdminCall, AdminCallBuilder},
            utils::send_tx,
        },
        dev::commands::upgrades::{
            args::chain::{ChainUpgradeParams, DefaultChainUpgradeArgs, UpgradeArgsInner},
            types::UpgradeVersion,
            utils::{print_error, set_upgrade_timestamp_calldata},
        },
    },
    utils::addresses::apply_l1_to_l2_alias,
};

#[derive(Debug, Default)]
pub struct FetchedChainInfo {
    pub hyperchain_addr: Address,
    pub chain_admin_addr: Address,
    pub gw_hyperchain_addr: Address,
    pub l1_asset_router_proxy: Address,
    pub settlement_layer: u64,
}

async fn verify_next_batch_new_version(
    batch_number: u32,
    main_node_client: &DynClient<L2>,
    upgrade_versions: UpgradeVersion,
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
    match upgrade_versions {
        UpgradeVersion::V28_1Vk | UpgradeVersion::V28_1VkEra => {
            ensure!(
                protocol_version >= ProtocolVersionId::Version28,
                "THe block does not yet contain the v28 upgrade"
            )
        }
        _ => ensure!(
            protocol_version >= ProtocolVersionId::Version29,
            "THe block does not yet contain the v29  upgrade"
        ),
    }

    Ok(())
}

pub async fn check_chain_readiness(
    l1_rpc_url: String,
    l2_rpc_url: String,
    gw_rpc_url: Option<String>,
    l2_chain_id: u64,
    gw_chain_id: Option<u64>,
    settlement_layer: u64,
    upgrade_versions: UpgradeVersion,
) -> anyhow::Result<()> {
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;

    let l2_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    if Some(settlement_layer) == gw_chain_id {
        // GW
        let gw_client = get_ethers_provider(
            &gw_rpc_url.context("Gw Rpc Url is required for gateway based chains")?,
        )?;
        let diamond_proxy_addr = (BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_client.clone()))
            .get_zk_chain(l2_chain_id.into())
            .await?;
        let zkchain = ZkChainAbi::new(diamond_proxy_addr, gw_client.clone());
        let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
        let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

        verify_next_batch_new_version(batches_committed, &l2_client, upgrade_versions).await?;
        verify_next_batch_new_version(batches_verified, &l2_client, upgrade_versions).await?;
    } else {
        // L1
        let diamond_proxy_addr = l2_client.get_main_l1_contract().await?;

        let zkchain = ZkChainAbi::new(diamond_proxy_addr, l1_provider.clone());
        let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
        let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

        verify_next_batch_new_version(batches_committed, &l2_client, upgrade_versions).await?;
        verify_next_batch_new_version(batches_verified, &l2_client, upgrade_versions).await?;
    }

    Ok(())
}

pub async fn fetch_chain_info(
    upgrade_info: &UpgradeInfo,
    args: &UpgradeArgsInner,
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

    let gw_hyperchain_addr = if settlement_layer != l1_provider.get_chainid().await? {
        let gw_client =
            get_ethers_provider(args.gw_rpc_url.as_ref().expect("gw_rpc_url is required"))?;

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

        gw_zkchain_addr
    } else {
        Address::zero()
    };

    Ok(FetchedChainInfo {
        hyperchain_addr: zkchain_addr,
        chain_admin_addr,
        gw_hyperchain_addr,
        l1_asset_router_proxy,
        settlement_layer: settlement_layer.as_u64(),
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpgradeInfo {
    // Information about pre-upgrade contracts.
    pub(crate) l1_chain_id: u32,
    pub(crate) gateway_chain_id: u32,
    pub(crate) deployed_addresses: DeployedAddresses,
    pub(crate) contracts_config: ContractsConfig,
    pub(crate) gateway: Gateway,

    // Information from upgrade
    pub(crate) chain_upgrade_diamond_cut: Bytes,
}

impl FileConfigTrait for UpgradeInfo {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractsConfig {
    pub(crate) new_protocol_version: u64,
    pub(crate) old_protocol_version: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployedAddresses {
    pub(crate) bridgehub: BridgehubAddresses,
    pub(crate) validator_timelock_addr: Address,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgehubAddresses {
    pub(crate) bridgehub_proxy_addr: Address,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Gateway {
    pub(crate) gateway_state_transition: GatewayStateTransition,
    pub(crate) diamond_cut_data: Bytes,
    pub(crate) upgrade_cut_data: Bytes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayStateTransition {
    pub(crate) validator_timelock_addr: Address,
}

pub struct UpdatedValidators {
    pub operator: Option<Address>,
    pub blob_operator: Option<Address>,
}

#[derive(Default)]
pub struct AdditionalUpgradeParams {
    pub updated_validators: Option<UpdatedValidators>,
}

pub(crate) async fn run_chain_upgrade(
    shell: &Shell,
    args_input: ChainUpgradeParams,
    additional: AdditionalUpgradeParams,
    run_upgrade: bool,
    upgrade_version: UpgradeVersion,
) -> anyhow::Result<()> {
    let forge_args = &Default::default();
    let contracts_foundry_path = ZkStackConfig::from_file(shell)?.path_to_l1_foundry();
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let mut args = args_input.clone().fill_if_empty(shell).await?;
    if args.upgrade_description_path.is_none() {
        args.upgrade_description_path = Some(
            chain_config
                .contracts_path()
                .join(upgrade_version.get_default_upgrade_description_path())
                .to_string_lossy()
                .to_string(),
        );
    }

    // 0. Read the GatewayUpgradeInfo
    let upgrade_info = UpgradeInfo::read(
        shell,
        args.clone()
            .upgrade_description_path
            .expect("upgrade_description_path is required"),
    )?;
    logger::info("upgrade_info: ");

    // 1. Update all the configs
    let chain_info = fetch_chain_info(&upgrade_info, &args.clone().into()).await?;
    logger::info(format!("chain_info: {:?}", chain_info));

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
    logger::info(serde_json::to_string_pretty(&set_timestamp_call)?);

    if !args.force_display_finalization_params.unwrap_or_default() {
        let chain_readiness = check_chain_readiness(
            args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            args.l2_rpc_url.clone().expect("l2_rpc_url is required"),
            args.gw_rpc_url.clone(),
            args.chain_id.expect("chain_id is required"),
            args.gw_chain_id,
            chain_info.settlement_layer,
            upgrade_version,
        )
        .await;

        if let Err(err) = chain_readiness {
            print_error(err);
            return Ok(());
        };
    }

    let (calldata, total_value) = if chain_info.settlement_layer == args.gw_chain_id.unwrap() {
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
                &contracts_foundry_path,
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
                args_input
                    .refund_recipient
                    .context("refund_recipient is required")?
                    .parse()
                    .context("refund recipient is not a valid address")?,
                upgrade_info.gateway.upgrade_cut_data.0.into(),
                args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            )
            .await;

        // v29: enable_validator_via_gateway for operator
        if let Some(validators) = &additional.updated_validators {
            let operator = validators.operator.context("operator is required")?;
            let enable_validator_calls = crate::admin_functions::enable_validator_via_gateway(
                shell,
                forge_args,
                &contracts_foundry_path,
                crate::admin_functions::AdminScriptMode::OnlySave,
                upgrade_info
                    .deployed_addresses
                    .bridgehub
                    .bridgehub_proxy_addr,
                args.l1_gas_price.expect("l1_gas_price is required").into(),
                args.chain_id.expect("chain_id is required"),
                args.gw_chain_id.expect("gw_chain_id is required"),
                operator,
                upgrade_info
                    .gateway
                    .gateway_state_transition
                    .validator_timelock_addr,
                operator,
                args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            )
            .await?;
            admin_calls_gw.extend_with_calls(enable_validator_calls.calls);
        }

        admin_calls_gw.display();

        let (gw_chain_admin_calldata, total_value) = admin_calls_gw.compile_full_calldata();

        logger::info(format!(
            "Full calldata to call `ChainAdmin` with : {}\nTotal value: {}",
            hex::encode(&gw_chain_admin_calldata),
            total_value,
        ));
        (gw_chain_admin_calldata, total_value)
    } else {
        let mut admin_calls_finalize = AdminCallBuilder::new(vec![]);

        admin_calls_finalize.append_execute_upgrade(
            chain_info.hyperchain_addr,
            upgrade_info.contracts_config.old_protocol_version,
            upgrade_info.chain_upgrade_diamond_cut.clone(),
        );

        // v29: enable_validator for operator and blob_operator
        if let Some(validators) = &additional.updated_validators {
            for validator in [
                validators.operator.context("operator is required")?,
                validators
                    .blob_operator
                    .context("blob_operator is required")?,
            ] {
                let enable_validator_calls = crate::admin_functions::enable_validator(
                    shell,
                    forge_args,
                    &contracts_foundry_path,
                    crate::admin_functions::AdminScriptMode::OnlySave,
                    upgrade_info
                        .deployed_addresses
                        .bridgehub
                        .bridgehub_proxy_addr,
                    args.chain_id.expect("chain_id is required"),
                    validator,
                    upgrade_info.deployed_addresses.validator_timelock_addr,
                    args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
                )
                .await?;
                admin_calls_finalize.extend_with_calls(enable_validator_calls.calls);
            }
        }

        admin_calls_finalize.display();

        let (chain_admin_calldata, total_value) = admin_calls_finalize.compile_full_calldata();

        logger::info(format!(
            "Full calldata to call `ChainAdmin` with : {}\nTotal value: {}",
            hex::encode(&chain_admin_calldata),
            total_value,
        ));
        (chain_admin_calldata, total_value)
    };

    if run_upgrade {
        let chain_config = ZkStackConfig::current_chain(shell)?;
        logger::info("Running upgrade");

        let receipt1 = send_tx(
            chain_info.chain_admin_addr,
            set_timestamp_call.data,
            U256::from(0),
            args.l1_rpc_url.clone().unwrap(),
            chain_config
                .get_wallets_config()?
                .governor
                .private_key_h256()
                .unwrap(),
            "set timestamp for upgrade",
        )
        .await?;
        logger::info("Set upgrade timestamp successfully!");
        logger::info(format!("receipt: {:#?}", receipt1));

        logger::info("Starting the migration!");
        let receipt = send_tx(
            chain_info.chain_admin_addr,
            calldata,
            total_value,
            args.l1_rpc_url.clone().unwrap(),
            chain_config
                .get_wallets_config()?
                .governor
                .private_key_h256()
                .unwrap(),
            "finalize upgrade",
        )
        .await?;
        logger::info("Upgrade completed successfully!");
        logger::info(format!("receipt: {:#?}", receipt));
    }

    Ok(())
}

pub(crate) async fn run(
    shell: &Shell,
    args_input: DefaultChainUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    run_chain_upgrade(
        shell,
        args_input.params.clone(),
        AdditionalUpgradeParams::default(),
        run_upgrade,
        args_input.upgrade_version,
    )
    .await
}
