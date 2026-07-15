use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::forge::ForgeScriptArgs;
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};

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
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, default_value_t = false)]
    pub only_save_calldata: bool,
}

pub async fn run(
    _args: ManageDepositsArgs,
    shell: &Shell,
    option: ManageDepositsOption,
) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;
    let chain_id = chain_config.chain_id;

    let contracts_config = chain_config.get_contracts_config()?;
    let bridgehub_address = contracts_config.ecosystem_contracts.bridgehub_proxy_addr;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    let mode = if _args.only_save_calldata {
        AdminScriptMode::OnlySave
    } else {
        AdminScriptMode::Broadcast(chain_config.get_wallets_config()?.governor)
    };

    let result = match option {
        ManageDepositsOption::PauseDeposits => {
            pause_deposits_before_initiating_migration(
                shell,
                &Default::default(),
                &chain_config.path_to_foundry_scripts(),
                mode,
                chain_id.as_u64(),
                bridgehub_address,
                l1_rpc_url,
            )
            .await?
        }
        ManageDepositsOption::UnpauseDeposits => {
            unpause_deposits(
                shell,
                &Default::default(),
                &chain_config.path_to_foundry_scripts(),
                mode,
                chain_id.as_u64(),
                bridgehub_address,
                l1_rpc_url,
            )
            .await?
        }
    };

    if _args.only_save_calldata {
        display_admin_script_output(result);
    }

    Ok(())
}
