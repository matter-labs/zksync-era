use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{ChainConfig, EcosystemConfig};

use crate::{
    commands::external_node::{args::run::RunExternalNodeArgs, init},
    external_node::RunExternalNode,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_STARTING_EN},
};

pub async fn run(shell: &Shell, args: RunExternalNodeArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

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
