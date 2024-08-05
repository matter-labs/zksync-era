use args::{prepare_configs::PrepareConfigArgs, run::RunExternalNodeArgs};
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;

mod args;
mod init;
mod prepare_configs;
mod run;

#[derive(Debug, Serialize, Deserialize, Parser)]
pub enum ExternalNodeCommands {
    Configs(PrepareConfigArgs),
    Init,
    Run(RunExternalNodeArgs),
}

pub async fn run(shell: &Shell, commands: ExternalNodeCommands) -> anyhow::Result<()> {
    match commands {
        ExternalNodeCommands::Configs(args) => prepare_configs::run(shell, args),
        ExternalNodeCommands::Init => init::run(shell).await,
        ExternalNodeCommands::Run(args) => run::run(shell, args).await,
    }
}
