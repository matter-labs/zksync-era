use clap::Subcommand;
use xshell::Shell;

pub mod build;

#[derive(Subcommand, Debug)]
pub enum ContractCommands {
    /// Build contracts
    Build,
}

pub(crate) async fn run(shell: &Shell, args: ContractCommands) -> anyhow::Result<()> {
    match args {
        ContractCommands::Build => build::run(shell).await,
    }
}
