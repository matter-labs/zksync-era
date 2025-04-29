use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zksync_types::Address;

use super::utils::{display_admin_script_output, get_default_foundry_path};
use crate::admin_functions::{set_transaction_filterer, AdminScriptMode};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct SetTransactionFiltererArgs {
    /// Gateway transaction filterer
    pub transaction_filterer: Address,

    /// Bridgehub address
    pub bridgehub_address: Address,

    /// The address of the ZK chain diamond proxy
    pub chain_id: u64,

    pub l1_rpc_url: String,
}

pub async fn run(shell: &Shell, args: SetTransactionFiltererArgs) -> anyhow::Result<()> {
    let result = set_transaction_filterer(
        shell,
        &Default::default(),
        &get_default_foundry_path(shell)?,
        AdminScriptMode::OnlySave,
        args.chain_id,
        args.bridgehub_address,
        args.transaction_filterer,
        args.l1_rpc_url,
    )
    .await?;

    display_admin_script_output(result);

    Ok(())
}
