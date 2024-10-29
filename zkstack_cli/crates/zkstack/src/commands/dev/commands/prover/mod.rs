use clap::Subcommand;
use xshell::Shell;

mod args;
pub mod info;
pub mod insert_batch;
pub mod insert_version;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    Info,
    InsertBatch(args::insert_batch::InsertBatchArgs),
    InsertVersion(args::insert_version::InsertVersionArgs),
}

pub async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Info => info::run(shell).await,
        ProverCommands::InsertBatch(args) => insert_batch::run(shell, args).await,
        ProverCommands::InsertVersion(args) => insert_version::run(shell, args).await,
    }
}
