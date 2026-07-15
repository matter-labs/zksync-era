use std::path::PathBuf;

use anyhow::{bail, Context};
use ethers::utils::hex;
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
    protocol_version::ProtocolVersionId, web3::Bytes, Address, L1BatchNumber, U256,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::ZksNamespaceClient,
};

use crate::{
    abi::{BridgehubAbi, IChainTypeManagerAbi, ZkChainAbi},
    commands::{
        chain::{
            admin_call_builder::{AdminCall, AdminCallBuilder},
            utils::send_tx,
        },
        dev::commands::upgrades::{
            args::chain::{ChainUpgradeParams, DefaultChainUpgradeArgs, UpgradeArgsInner},
            types::UpgradeVersion,
            utils::{
                print_error, server_notifier_set_upgrade_timestamp_calldata,
                set_upgrade_timestamp_calldata,
            },
        },
    },
    utils::protocol_version::get_minor_protocol_version,
};

#[derive(Debug, Default)]
pub struct FetchedChainInfo {
    pub hyperchain_addr: Address,
    pub chain_admin_addr: Address,
    pub server_notifier_addr: Address,
}

async fn verify_next_batch_new_version(
    batch_number: u32,
    main_node_client: &DynClient<L2>,
    _upgrade_versions: UpgradeVersion,
) -> anyhow::Result<()> {
    let (_, _right_bound) = main_node_client
        .get_l2_block_range(L1BatchNumber(batch_number))
        .await?
        .context("Range must be present for a batch")?;

    Ok(())
}

pub async fn check_chain_readiness(
    l1_rpc_url: String,
    l2_rpc_url: String,
    l2_chain_id: u64,
    upgrade_versions: UpgradeVersion,
) -> anyhow::Result<()> {
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;

    let l2_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    let diamond_proxy_addr = l2_client.get_main_l1_contract().await?;

    let zkchain = ZkChainAbi::new(diamond_proxy_addr, l1_provider.clone());
    let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
    let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

    verify_next_batch_new_version(batches_committed, &l2_client, upgrade_versions).await?;
    verify_next_batch_new_version(batches_verified, &l2_client, upgrade_versions).await?;

    if matches!(upgrade_versions, UpgradeVersion::V29InteropAFf) {
        let batches_executed = zkchain.get_total_batches_executed().await?.as_u32();
        verify_next_batch_new_version(batches_executed, &l2_client, upgrade_versions).await?;
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

    let chain_type_manager_addr = bridgehub.chain_type_manager(chain_id).await?;
    let zkchain = ZkChainAbi::new(zkchain_addr, l1_provider.clone());
    let chain_type_manager =
        IChainTypeManagerAbi::new(chain_type_manager_addr, l1_provider.clone());

    let chain_admin_addr = zkchain.get_admin().await?;
    let server_notifier_addr = chain_type_manager.server_notifier_address().await?;

    Ok(FetchedChainInfo {
        hyperchain_addr: zkchain_addr,
        chain_admin_addr,
        server_notifier_addr,
    })
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpgradeInfo {
    pub(crate) deployed_addresses: DeployedAddresses,

    pub(crate) contracts_config: ContractsConfig,

    // Information from upgrade
    #[serde(default)]
    pub(crate) chain_upgrade_diamond_cut: Bytes,
    #[serde(default)]
    pub(crate) chain_upgrade_diamond_cut_file: Option<PathBuf>,
}

impl UpgradeInfo {
    /// Load the diamond cut data from the file if it hasn't been loaded yet
    pub fn load_diamond_cut(&mut self) -> anyhow::Result<()> {
        if self.chain_upgrade_diamond_cut.0.is_empty() {
            if let Some(ref file_path) = self.chain_upgrade_diamond_cut_file {
                let hex_string = std::fs::read_to_string(file_path)?;
                let hex_trimmed = hex_string.trim().trim_start_matches("0x");
                let bytes = hex::decode(hex_trimmed)?;
                self.chain_upgrade_diamond_cut = Bytes(bytes);
            }
        }
        Ok(())
    }
}

impl FileConfigTrait for UpgradeInfo {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployedAddresses {
    pub(crate) bridgehub: BridgehubAddresses,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgehubAddresses {
    pub(crate) bridgehub_proxy_addr: Address,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractsConfig {
    pub(crate) new_protocol_version: u64,
    pub(crate) old_protocol_version: u64,
}

pub(crate) async fn run_chain_upgrade(
    shell: &Shell,
    args_input: ChainUpgradeParams,
    run_upgrade: bool,
    upgrade_version: UpgradeVersion,
) -> anyhow::Result<()> {
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
    let mut upgrade_info = UpgradeInfo::read(
        shell,
        args.clone()
            .upgrade_description_path
            .expect("upgrade_description_path is required"),
    )?;

    // Load the diamond cut data from file
    upgrade_info.load_diamond_cut()?;

    logger::info("upgrade_info: ");

    // 1. Update all the configs
    let chain_info = fetch_chain_info(&upgrade_info, &args.clone().into()).await?;
    logger::info(format!("chain_info: {:?}", chain_info));

    // 2. Generate calldata
    let chain_id = args.chain_id.expect("chain_id is required");
    let server_upgrade_timestamp = args
        .server_upgrade_timestamp
        .expect("server_upgrade_timestamp is required");
    let schedule_calldata = set_upgrade_timestamp_calldata(
        upgrade_info.contracts_config.new_protocol_version,
        server_upgrade_timestamp,
    );

    let set_timestamp_call = AdminCall {
        description: "Calldata to schedule upgrade".to_string(),
        data: schedule_calldata,
        target: chain_info.chain_admin_addr,
        value: U256::zero(),
    };
    logger::info(serde_json::to_string_pretty(&set_timestamp_call)?);

    let server_notifier_set_timestamp_call = AdminCall {
        description: "Calldata to notify server of scheduled upgrade".to_string(),
        data: server_notifier_set_upgrade_timestamp_calldata(chain_id, server_upgrade_timestamp),
        target: chain_info.server_notifier_addr,
        value: U256::zero(),
    };

    if !args.force_display_finalization_params.unwrap_or_default() {
        let chain_readiness = check_chain_readiness(
            args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            args.l2_rpc_url.clone().expect("l2_rpc_url is required"),
            chain_id,
            upgrade_version,
        )
        .await;

        if let Err(err) = chain_readiness {
            print_error(err);
            return Ok(());
        };
    }

    // ServerNotifier notification (call #2): a v31+ feature, so emitted only for v31+ targets
    // (the deployed `ServerNotifier` on pre-v31 CTMs lacks the function). Sent as its own decoupled
    // `ChainAdmin` multicall so it can never roll back the upgrade execution (call #3).
    let new_minor_version = get_minor_protocol_version(U256::from(
        upgrade_info.contracts_config.new_protocol_version,
    ))?;

    let server_notifier_calldata = if new_minor_version >= ProtocolVersionId::Version31 {
        let server_notifier_calls = AdminCallBuilder::new(vec![server_notifier_set_timestamp_call]);
        server_notifier_calls.display();
        let (data, value) = server_notifier_calls.compile_full_calldata();
        logger::info(format!(
            "Calldata to call `ChainAdmin` with for the `ServerNotifier` notification: {}\nTotal value: {}",
            hex::encode(&data),
            value,
        ));
        Some((data, value))
    } else {
        logger::info(format!(
            "Skipping `ServerNotifier` notification: target protocol version {new_minor_version:?} is below v31."
        ));
        None
    };

    // Upgrade execution (call #3): its own `ChainAdmin` multicall.
    let mut execute_upgrade_calls = AdminCallBuilder::new(vec![]);
    execute_upgrade_calls.append_execute_upgrade(
        chain_info.hyperchain_addr,
        upgrade_info.contracts_config.old_protocol_version,
        upgrade_info.chain_upgrade_diamond_cut.clone(),
    );
    execute_upgrade_calls.display();

    let (calldata, total_value) = if execute_upgrade_calls.is_empty() {
        logger::info("No calls to execute for direct upgrade");
        (vec![], U256::zero())
    } else {
        let (data, value) = execute_upgrade_calls.compile_full_calldata();
        logger::info(format!(
            "Full calldata to call `ChainAdmin` with for the upgrade execution: {}\nTotal value: {}",
            hex::encode(&data),
            value,
        ));
        (data, value)
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

        // Notify the post-upgrade server via `ServerNotifier` (call #2), unless skipped.
        if let Some((server_notifier_calldata, server_notifier_value)) = server_notifier_calldata {
            logger::info("Notifying `ServerNotifier` of the scheduled upgrade");
            let receipt = send_tx(
                chain_info.chain_admin_addr,
                server_notifier_calldata,
                server_notifier_value,
                args.l1_rpc_url.clone().unwrap(),
                chain_config
                    .get_wallets_config()?
                    .governor
                    .private_key_h256()
                    .unwrap(),
                "notify server notifier",
            )
            .await?;
            logger::info("Notified `ServerNotifier` successfully!");
            logger::info(format!("receipt: {:#?}", receipt));
        } else {
            logger::info("Skipping `ServerNotifier` notification");
        }

        // Only run migration if there are calls to execute
        if !calldata.is_empty() {
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
        } else {
            logger::info("Skipping migration (no calls to execute)");
        }
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
        run_upgrade,
        args_input.upgrade_version,
    )
    .await
}
