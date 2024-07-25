use args::{init::ProverInitArgs, init_bellman_cuda::InitBellmanCudaArgs, run::ProverRunArgs};
use clap::Subcommand;
use xshell::Shell;

mod args;
mod gcs;
mod generate_sk;
mod init;
mod init_bellman_cuda;
mod run;
mod utils;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    Init(Box<ProverInitArgs>),
    /// Generate setup keys
    #[command(alias = "sk")]
    GenerateSK,
    /// Run prover
    Run(ProverRunArgs),
    /// Initialize bellman-cuda
    #[command(alias = "cuda")]
    InitBellmanCuda(Box<InitBellmanCudaArgs>),
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Init(args) => init::run(*args, shell).await,
        ProverCommands::GenerateSK => generate_sk::run(shell).await,
        ProverCommands::Run(args) => run::run(args, shell).await,
        ProverCommands::InitBellmanCuda(args) => init_bellman_cuda::run(shell, *args).await,
    }
}
