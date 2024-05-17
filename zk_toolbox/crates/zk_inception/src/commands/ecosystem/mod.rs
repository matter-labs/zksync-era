use clap::Subcommand;
use xshell::Shell;

use crate::commands::ecosystem::args::{
    change_default::ChangeDefaultHyperchain, create::EcosystemCreateArgs, init::EcosystemInitArgs,
};

mod args;
mod change_default;
mod create;
pub mod create_configs;
mod init;

#[derive(Subcommand, Debug)]
pub enum EcosystemCommands {
    /// Create a new ecosystem and hyperchain,
    /// setting necessary configurations for later initialization
    Create(EcosystemCreateArgs),
    /// Initialize ecosystem and hyperchain,
    /// deploying necessary contracts and performing on-chain operations
    Init(EcosystemInitArgs),
    /// Change the default hyperchain
    ChangeDefaultHyperchain(ChangeDefaultHyperchain),
}

pub(crate) async fn run(shell: &Shell, args: EcosystemCommands) -> anyhow::Result<()> {
    match args {
        EcosystemCommands::Create(args) => create::run(args, shell),
        EcosystemCommands::Init(args) => init::run(args, shell).await,
        EcosystemCommands::ChangeDefaultHyperchain(args) => change_default::run(args, shell),
    }
}
