use args::init::InitArgs;
use clap::Subcommand;
use xshell::Shell;
mod args;
mod init;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    Init(InitArgs),
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Init(args) => init::run(args, shell).await,
    }
}
