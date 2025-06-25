use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{ChainConfig, ZkStackConfig};

use crate::{
    commands::external_node::{args::run::RunExternalNodeArgs, init},
    external_node::RunExternalNode,
    messages::MSG_STARTING_EN,
};

pub async fn run(shell: &Shell, args: RunExternalNodeArgs) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    logger::info(MSG_STARTING_EN);

    run_external_node(args, &chain_config, shell).await?;

    Ok(())
}

async fn run_external_node(
    args: RunExternalNodeArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    if args.reinit {
        init::init(shell, chain_config).await?
    }
    let server = RunExternalNode::new(args.components.clone(), chain_config)?;
    server.run(shell, args.additional_args.clone())
}
