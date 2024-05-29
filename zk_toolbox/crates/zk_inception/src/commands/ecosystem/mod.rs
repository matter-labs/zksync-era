use clap::Subcommand;
use xshell::Shell;

use crate::{
    commands::ecosystem::args::{
        change_default::ChangeDefaultChain, create::EcosystemCreateArgs, init::EcosystemInitArgs,
    },
    configs::{EcosystemConfig, EcosystemConfigFromFileError},
};

mod args;
mod change_default;
mod create;
pub mod create_configs;
mod init;

#[derive(Subcommand, Debug)]
pub enum EcosystemCommands {
    /// Create a new ecosystem and chain,
    /// setting necessary configurations for later initialization
    Create(EcosystemCreateArgs),
    /// Initialize ecosystem and chain,
    /// deploying necessary contracts and performing on-chain operations
    Init(EcosystemInitArgs),
    /// Change the default chain
    ChangeDefaultChain(ChangeDefaultChain),
}

pub(crate) async fn run(
    shell: &Shell,
    args: EcosystemCommands,
    ecosystem_config: Result<EcosystemConfig, EcosystemConfigFromFileError>,
) -> anyhow::Result<()> {
    match args {
        EcosystemCommands::Create(args) => create::run(args, shell, ecosystem_config),
        EcosystemCommands::Init(args) => init::run(args, shell, ecosystem_config?).await,
        EcosystemCommands::ChangeDefaultChain(args) => {
            change_default::run(args, shell, ecosystem_config?)
        }
    }
}
