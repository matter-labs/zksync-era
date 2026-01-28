use clap::Parser;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};
use zksync_basic_types::U256;

use crate::{messages::MSG_INTEROP_FEE_SET, set_interop_fee::set_interop_fee};

#[derive(Debug, Parser)]
pub struct SetInteropFeeArgs {
    #[clap(long)]
    pub fee: u64,
    #[clap(flatten)]
    pub forge_args: ForgeScriptArgs,
}

pub async fn run(args: SetInteropFeeArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let contracts = chain_config.get_contracts_config()?;
    let secrets = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets.l1_rpc_url()?;

    set_interop_fee(
        shell,
        &chain_config.path_to_foundry_scripts(),
        contracts.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts.l1.diamond_proxy_addr,
        U256::from(args.fee),
        &args.forge_args,
        l1_rpc_url,
    )
    .await?;
    logger::success(MSG_INTEROP_FEE_SET);
    Ok(())
}
