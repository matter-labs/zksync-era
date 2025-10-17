use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::forge::{ForgeArgs, ForgeRunner};
use zkstack_cli_config::ZkStackConfigTrait;
use zksync_types::Address;

use super::utils::display_admin_script_output;
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

    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeArgs,
}

pub async fn run(shell: &Shell, args: SetTransactionFiltererArgs) -> anyhow::Result<()> {
    let chain_config = zkstack_cli_config::ZkStackConfig::current_chain(shell)
        .context("Failed to load the current chain configuration")?;
    let mut runner = ForgeRunner::new(args.forge_args.runner.clone());
    let result = set_transaction_filterer(
        shell,
        &mut runner,
        &args.forge_args.script,
        &chain_config.path_to_foundry_scripts(),
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
