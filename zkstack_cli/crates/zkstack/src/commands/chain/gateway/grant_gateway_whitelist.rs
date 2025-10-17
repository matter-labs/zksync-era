use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::forge::ForgeRunner;
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};
use zksync_types::Address;

use crate::{
    admin_functions::{grant_gateway_whitelist, AdminScriptMode},
    commands::chain::utils::display_admin_script_output,
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct GrantGatewayWhitelistCalldataArgs {
    #[clap(long)]
    pub bridgehub_addr: Address,
    #[clap(long)]
    pub gateway_chain_id: u64,
    #[clap(long)]
    pub l1_rpc_url: String,
    #[clap(long)]
    pub grantees: Vec<Address>,
}

pub async fn run(shell: &Shell, args: GrantGatewayWhitelistCalldataArgs) -> anyhow::Result<()> {
    let foundry_path = ZkStackConfig::from_file(shell)?.path_to_foundry_scripts();
    let mut runner = ForgeRunner::default();
    let result = grant_gateway_whitelist(
        shell,
        &mut runner,
        // We do not care about forge args that much here, since
        // we only need to obtain the calldata
        &Default::default(),
        &foundry_path,
        AdminScriptMode::OnlySave,
        args.gateway_chain_id,
        args.bridgehub_addr,
        args.grantees,
        args.l1_rpc_url.clone(),
    )
    .await?;

    display_admin_script_output(result);

    Ok(())
}
