use args::run::ProverRunArgs;
use args::init::ProverInitArgs;
use clap::Subcommand;
use xshell::Shell;
mod args;
mod generate_sk;
mod run;
mod init;
mod utils;
mod gcs;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    Init(ProverInitArgs),
    /// Generate setup keys
    GenerateSK,
    /// Run prover
    Run(ProverRunArgs),
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Init(args) => init::run(args, shell).await,
        ProverCommands::GenerateSK => generate_sk::run(shell).await,
        ProverCommands::Run(args) => run::run(args, shell).await,
    }
}
