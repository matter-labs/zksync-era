use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_config::ZkStackConfigTrait;
use zksync_types::Address;

use super::utils::display_admin_script_output;
use crate::admin_functions::{
    pause_deposits_before_initiating_migration, unpause_deposits, AdminScriptMode,
};

pub enum ManageDepositsOption {
    PauseDeposits,
    UnpauseDeposits,
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct ManageDepositsArgs {
    /// Bridgehub address
    pub bridgehub_address: Address,
    /// The chain ID of the ZK chain
    pub chain_id: u64,
    pub l1_rpc_url: String,
}

pub async fn run(
    args: ManageDepositsArgs,
    shell: &Shell,
    option: ManageDepositsOption,
) -> anyhow::Result<()> {
    let chain_config = zkstack_cli_config::ZkStackConfig::current_chain(shell)
        .context("Failed to load the current chain configuration")?;

    let result = match option {
        ManageDepositsOption::PauseDeposits => {
            pause_deposits_before_initiating_migration(
                shell,
                &Default::default(),
                &chain_config.path_to_foundry_scripts(),
                AdminScriptMode::OnlySave,
                args.chain_id,
                args.bridgehub_address,
                args.l1_rpc_url,
            )
            .await?
        }
        ManageDepositsOption::UnpauseDeposits => {
            unpause_deposits(
                shell,
                &Default::default(),
                &chain_config.path_to_foundry_scripts(),
                AdminScriptMode::OnlySave,
                args.chain_id,
                args.bridgehub_address,
                args.l1_rpc_url,
            )
            .await?
        }
    };

    display_admin_script_output(result);

    Ok(())
}
