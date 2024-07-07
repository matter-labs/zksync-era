<<<<<<< HEAD
use clap::Subcommand;
use xshell::Shell;
mod generate_sk;
=======
use args::init::ProverInitArgs;
use clap::Subcommand;
use xshell::Shell;

mod args;
mod gcs;
mod generate_sk;
mod init;
mod utils;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    Init(Box<ProverInitArgs>),
    /// Generate setup keys
    GenerateSK,
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Init(args) => init::run(*args, shell).await,
        ProverCommands::GenerateSK => generate_sk::run(shell).await,
    }
}
