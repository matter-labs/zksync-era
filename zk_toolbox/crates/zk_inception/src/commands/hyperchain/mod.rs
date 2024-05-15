pub(crate) mod args;
mod create;
pub mod deploy_paymaster;
pub mod genesis;
pub(crate) mod init;
mod initialize_bridges;

pub(crate) use args::create::HyperchainCreateArgsFinal;
use clap::Subcommand;
use common::forge::ForgeScriptArgs;
pub(crate) use create::create_hyperchain_inner;
use xshell::Shell;

use crate::commands::hyperchain::args::{
    create::HyperchainCreateArgs, genesis::GenesisArgs, init::InitArgs,
};

#[derive(Subcommand, Debug)]
pub enum HyperchainCommands {
    /// Create a new hyperchain, setting the necessary configurations for later initialization
    Create(HyperchainCreateArgs),
    /// Initialize hyperchain, deploying necessary contracts and performing on-chain operations
    Init(InitArgs),
    /// Run server genesis
    Genesis(GenesisArgs),
    /// Initialize bridges on l2
    InitializeBridges(ForgeScriptArgs),
    /// Initialize bridges on l2
    DeployPaymaster(ForgeScriptArgs),
}

pub(crate) async fn run(shell: &Shell, args: HyperchainCommands) -> anyhow::Result<()> {
    match args {
        HyperchainCommands::Create(args) => create::run(args, shell),
        HyperchainCommands::Init(args) => init::run(args, shell).await,
        HyperchainCommands::Genesis(args) => genesis::run(args, shell).await,
        HyperchainCommands::InitializeBridges(args) => initialize_bridges::run(args, shell),
        HyperchainCommands::DeployPaymaster(args) => deploy_paymaster::run(args, shell),
    }
}
