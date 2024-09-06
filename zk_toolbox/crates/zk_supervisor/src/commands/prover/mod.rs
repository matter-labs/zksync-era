use clap::Subcommand;
use xshell::Shell;

pub mod info;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    Info,
}

pub async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Info => info::run(shell).await,
    }
}
