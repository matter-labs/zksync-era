use args::{init::ProverInitArgs, run::ProverRunArgs};
use clap::Subcommand;
use xshell::Shell;

mod args;
mod gcs;
mod generate_sk;
mod init;
mod run;
mod utils;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    Init(Box<ProverInitArgs>),
    /// Generate setup keys
    GenerateSK,
    /// Run prover
    Run(ProverRunArgs),
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Init(args) => init::run(*args, shell).await,
        ProverCommands::GenerateSK => generate_sk::run(shell).await,
        ProverCommands::Run(args) => run::run(args, shell).await,
    }
}
