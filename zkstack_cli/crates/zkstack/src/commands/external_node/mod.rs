use clap::Parser;
use xshell::Shell;

use self::args::{prepare_configs::PrepareConfigArgs, run::RunExternalNodeArgs};
use crate::commands::args::WaitArgs;

mod args;
mod build;
mod init;
mod prepare_configs;
mod run;
mod wait;

#[derive(Debug, Parser)]
pub enum ExternalNodeCommands {
    /// Prepare configs for EN
    Configs(PrepareConfigArgs),
    /// Init databases
    Init,
    /// Build external node
    Build,
    /// Run external node
    Run(RunExternalNodeArgs),
    /// Wait for external node to start
    Wait(WaitArgs),
}

pub async fn run(shell: &Shell, commands: ExternalNodeCommands) -> anyhow::Result<()> {
    match commands {
        ExternalNodeCommands::Configs(args) => prepare_configs::run(shell, args).await,
        ExternalNodeCommands::Init => init::run(shell).await,
        ExternalNodeCommands::Build => build::build(shell).await,
        ExternalNodeCommands::Run(args) => run::run(shell, args).await,
        ExternalNodeCommands::Wait(args) => wait::wait(shell, args).await,
    }
}
