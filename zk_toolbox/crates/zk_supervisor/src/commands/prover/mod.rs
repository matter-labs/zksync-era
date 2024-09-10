use clap::Subcommand;
use xshell::Shell;

mod args;
pub mod info;
pub mod insert_batch;
pub mod insert_version;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    Info,
    InsertBatch,
    InsertVersion,
}

pub async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Info => info::run(shell).await,
        ProverCommands::InsertBatch => insert_batch::run(shell).await,
        ProverCommands::InsertVersion => insert_version::run(shell).await,
    }
}
