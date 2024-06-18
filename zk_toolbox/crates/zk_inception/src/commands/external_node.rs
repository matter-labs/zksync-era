use anyhow::Context;
use common::{config::global_config, logger};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use crate::{
    commands::args::RunExternalNodeArgs,
    external_node::RunExternalNode,
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_STARTING_SERVER},
};

pub fn run(shell: &Shell, args: RunExternalNodeArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    logger::info(MSG_STARTING_SERVER);

    run_external_node(args, &chain_config, shell)?;

    Ok(())
}

fn run_external_node(
    args: RunExternalNodeArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    let server = RunExternalNode::new(args.components.clone(), chain_config)?;
    server.run(shell, args.additional_args.clone())
}
