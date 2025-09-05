use std::path::Path;

use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger};
use zksync_types::Address;

use super::{
    gateway_common::{
        get_gateway_migration_state, GatewayMigrationProgressState, MigrationDirection,
    },
    messages::message_for_gateway_migration_progress_state,
};
use crate::{
    admin_functions::{
        notify_server_migration_from_gateway, notify_server_migration_to_gateway, AdminScriptMode,
        AdminScriptOutput,
    },
    commands::chain::utils::{display_admin_script_output, get_default_foundry_path},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct NotifyServerCallsArgs {
    pub l1_bridgehub_addr: Address,
    pub l2_chain_id: u64,
    pub l1_rpc_url: String,
}

pub async fn get_notify_server_calls(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    forge_path: &Path,
    args: NotifyServerCallsArgs,
    direction: MigrationDirection,
) -> anyhow::Result<AdminScriptOutput> {
    let admin_call_output = match direction {
        MigrationDirection::FromGateway => {
            notify_server_migration_from_gateway(
                shell,
                forge_args,
                forge_path,
                AdminScriptMode::OnlySave,
                args.l2_chain_id,
                args.l1_bridgehub_addr,
                args.l1_rpc_url,
            )
            .await
        }
        MigrationDirection::ToGateway => {
            notify_server_migration_to_gateway(
                shell,
                forge_args,
                forge_path,
                AdminScriptMode::OnlySave,
                args.l2_chain_id,
                args.l1_bridgehub_addr,
                args.l1_rpc_url,
            )
            .await
        }
    }?;

    Ok(admin_call_output)
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct NotifyServerCalldataArgs {
    #[clap(flatten)]
    pub params: NotifyServerCallsArgs,
    #[clap(long)]
    pub l2_rpc_url: Option<String>,
    #[clap(long)]
    pub gw_rpc_url: Option<String>,
    #[clap(long, default_missing_value = "false")]
    pub no_cross_check: bool,
}

pub async fn run(
    shell: &Shell,
    args: NotifyServerCalldataArgs,
    direction: MigrationDirection,
) -> anyhow::Result<()> {
    if !args.no_cross_check {
        let status = get_gateway_migration_state(
            args.params.l1_rpc_url.clone(),
            args.params.l1_bridgehub_addr,
            args.params.l2_chain_id,
            args.l2_rpc_url.context(
                "L2 RPC URL must be provided for cross checking the state with the server",
            )?,
            args.gw_rpc_url.context(
                "GW RPC URL must be provided for cross checking the state with the server",
            )?,
            direction,
        )
        .await?;

        match status {
            GatewayMigrationProgressState::NotStarted => {
                logger::info("Migration in this direction has not yet started. Preparing the calldata for the notification.");
            }
            _ => {
                anyhow::bail!(message_for_gateway_migration_progress_state(
                    status, direction
                ));
            }
        }
    }

    let result = get_notify_server_calls(
        shell,
        // We do not care about forge args that much here, since
        // we only need to obtain the calldata
        &Default::default(),
        &get_default_foundry_path(shell)?,
        args.params,
        direction,
    )
    .await?;

    display_admin_script_output(result);

    Ok(())
}
