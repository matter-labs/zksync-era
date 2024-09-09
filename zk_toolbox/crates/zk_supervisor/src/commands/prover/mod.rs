use clap::Subcommand;
use xshell::Shell;

pub mod cli;
pub mod info;
pub mod init_version;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    Info,
    Cli,
    InitVersion,
}

pub async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Info => info::run(shell).await,
        ProverCommands::Cli => cli::run(shell).await,
        ProverCommands::InitVersion => init_version::run(shell).await,
    }
}
