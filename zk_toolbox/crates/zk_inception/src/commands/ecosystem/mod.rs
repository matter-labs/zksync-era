use args::transaction::EcosystemTransactionArgs;
use clap::Subcommand;
use xshell::Shell;

use crate::commands::ecosystem::args::{
    change_default::ChangeDefaultChain, create::EcosystemCreateArgs, init::EcosystemInitArgs,
};

mod args;
mod change_default;
mod create;
pub mod create_configs;
pub(crate) mod init;
mod setup_observability;
pub(crate) mod transaction;
mod utils;

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EcosystemCommands {
    /// Create a new ecosystem and chain,
    /// setting necessary configurations for later initialization
    Create(EcosystemCreateArgs),
    /// Create transactions to build ecosystem contracts
    Transaction(EcosystemTransactionArgs),
    /// Initialize ecosystem and chain,
    /// deploying necessary contracts and performing on-chain operations
    Init(EcosystemInitArgs),
    /// Change the default chain
    #[command(alias = "cd")]
    ChangeDefaultChain(ChangeDefaultChain),
    /// Setup observability for the ecosystem,
    /// downloading Grafana dashboards from the era-observability repo
    #[command(alias = "obs")]
    SetupObservability,
}

pub(crate) async fn run(shell: &Shell, args: EcosystemCommands) -> anyhow::Result<()> {
    match args {
        EcosystemCommands::Create(args) => create::run(args, shell),
        EcosystemCommands::Transaction(args) => transaction::run(args, shell).await,
        EcosystemCommands::Init(args) => init::run(args, shell).await,
        EcosystemCommands::ChangeDefaultChain(args) => change_default::run(args, shell),
        EcosystemCommands::SetupObservability => setup_observability::run(shell),
    }
}
