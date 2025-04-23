use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::logger;
use zksync_types::Address;

use super::utils::{display_admin_script_output, get_default_foundry_path};
use crate::accept_ownership::{grant_gateway_whitelist, AdminScriptMode};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct GrantGatewayWhitelistCalldataArgs {
    pub bridgehub_addr: Address,

    pub gateway_chain_id: u64,

    pub l1_rpc_url: String,

    /// Who to grant the whitelist to.
    pub grantees: Vec<Address>,
}

pub async fn run(shell: &Shell, args: GrantGatewayWhitelistCalldataArgs) -> anyhow::Result<()> {
    let mut results = vec![];

    for grantee in args.grantees {
        let result = grant_gateway_whitelist(
            shell,
            // We do not care about forge args that much here, since
            // we only need to obtain the calldata
            &Default::default(),
            &get_default_foundry_path(shell)?,
            AdminScriptMode::OnlySave,
            args.gateway_chain_id,
            args.bridgehub_addr,
            grantee,
            args.l1_rpc_url.clone(),
        )
        .await?;

        results.push(result);
    }

    let Some(admin_call_output) = results.into_iter().reduce(|mut acc, x| {
        acc.extend(x).unwrap();
        acc
    }) else {
        logger::info("The list of grantees is empty");
        return Ok(());
    };

    display_admin_script_output(admin_call_output);

    Ok(())
}
