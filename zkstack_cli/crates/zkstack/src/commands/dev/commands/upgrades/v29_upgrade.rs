use ethers::utils::hex;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{traits::ReadConfig, EcosystemConfig};
use zksync_basic_types::U256;

use crate::{
    admin_functions::{enable_validator, enable_validator_via_gateway, AdminScriptMode},
    commands::{
        chain::{
            admin_call_builder::{AdminCall, AdminCallBuilder},
            utils::{get_default_foundry_path, send_tx},
        },
        dev::commands::upgrades::{
            args::{chain::UpgradeArgsInner, v29_chain::V29ChainUpgradeArgs},
            default_chain_upgrade::{check_chain_readiness, fetch_chain_info, UpgradeInfo},
            utils::{print_error, set_upgrade_timestamp_calldata},
        },
    },
};

pub(crate) async fn run(
    shell: &Shell,
    args_input: V29ChainUpgradeArgs,
    run_upgrade: bool,
) -> anyhow::Result<()> {
    let forge_args = &Default::default();
    let foundry_contracts_path = get_default_foundry_path(shell)?;
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let mut args = args_input.clone().fill_if_empty(shell).await?;
    if args.upgrade_description_path.is_none() {
        args.upgrade_description_path = Some(
            ecosystem_config
                .link_to_code
                .join(
                    args_input
                        .upgrade_version
                        .get_default_upgrade_description_path(),
                )
                .to_string_lossy()
                .to_string(),
        );
    }

    // 0. Read the GatewayUpgradeInfo
    let upgrade_info = UpgradeInfo::read(
        shell,
        &args
            .clone()
            .upgrade_description_path
            .expect("upgrade_description_path is required"),
    )?;
    logger::info("upgrade_info: ");

    // 1. Update all the configs
    let chain_info = fetch_chain_info(&upgrade_info, &UpgradeArgsInner::from(args.clone())).await?;
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
            args.upgrade_version,
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
                // TODO: the funds go to nowhere as this is not the aliased address.
                chain_info.chain_admin_addr,
                upgrade_info.gateway.upgrade_cut_data.0.into(),
                args.l1_rpc_url.clone().expect("l1_rpc_url is required"),
            )
            .await;

        for validator in [args.validator_1, args.validator_2] {
            let enable_validator_calls = enable_validator_via_gateway(
                shell,
                forge_args,
                &foundry_contracts_path,
                AdminScriptMode::OnlySave,
                upgrade_info
                    .deployed_addresses
                    .bridgehub
                    .bridgehub_proxy_addr,
                args.l1_gas_price.expect("l1_gas_price is required").into(),
                args.chain_id.expect("chain_id is required"),
                args.gw_chain_id.expect("gw_chain_id is required"),
                validator,
                upgrade_info
                    .gateway
                    .gateway_state_transition
                    .validator_timelock_addr,
                validator,
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

        for validator in [args.validator_1, args.validator_2] {
            let enable_validator_calls = enable_validator(
                shell,
                forge_args,
                &foundry_contracts_path,
                AdminScriptMode::OnlySave,
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
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let chain_config = ecosystem_config
            .load_current_chain()
            .expect("Chain not found");
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
