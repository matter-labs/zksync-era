pub(crate) use args::create::ChainCreateArgsFinal;
use clap::Subcommand;
use common::forge::ForgeScriptArgs;
pub(crate) use create::create_chain_inner;
use xshell::Shell;

use crate::commands::chain::args::{create::ChainCreateArgs, genesis::GenesisArgs, init::InitArgs};

pub(crate) mod args;
mod create;
pub mod deploy_paymaster;
pub mod genesis;
pub(crate) mod init;
mod initialize_bridges;

#[derive(Subcommand, Debug)]
pub enum ChainCommands {
    /// Create a new chain, setting the necessary configurations for later initialization
    Create(ChainCreateArgs),
    /// Initialize chain, deploying necessary contracts and performing on-chain operations
    Init(InitArgs),
    /// Run server genesis
    Genesis(GenesisArgs),
    /// Initialize bridges on l2
    InitializeBridges(ForgeScriptArgs),
    /// Initialize bridges on l2
    DeployPaymaster(ForgeScriptArgs),
}

pub(crate) async fn run(shell: &Shell, args: ChainCommands) -> anyhow::Result<()> {
    match args {
        ChainCommands::Create(args) => create::run(args, shell),
        ChainCommands::Init(args) => init::run(args, shell).await,
        ChainCommands::Genesis(args) => genesis::run(args, shell).await,
        ChainCommands::InitializeBridges(args) => initialize_bridges::run(args, shell).await,
        ChainCommands::DeployPaymaster(args) => deploy_paymaster::run(args, shell).await,
    }
}
