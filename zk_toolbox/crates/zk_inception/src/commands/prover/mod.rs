use clap::Subcommand;
use xshell::Shell;
mod generate_sk;
mod run;
mod utils;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    GenerateSK,
    /// Run prover
    Run,
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::GenerateSK => generate_sk::run(shell).await,
        ProverCommands::Run => run::run(shell).await,
    }
}
