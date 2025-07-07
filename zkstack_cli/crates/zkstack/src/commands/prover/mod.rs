use args::{
    compressor_keys::CompressorKeysArgs, init::ProverInitArgs,
    init_bellman_cuda::InitBellmanCudaArgs, run::ProverRunArgs,
};
use clap::Subcommand;
use xshell::Shell;

use crate::commands::prover::args::setup_keys::SetupKeysArgs;

mod args;
mod compressor_keys;
mod deploy_proving_network;
mod gcs;
mod init;
mod init_bellman_cuda;
mod run;
mod setup_keys;

#[derive(Subcommand, Debug)]
pub enum ProverCommands {
    /// Initialize prover
    Init(Box<ProverInitArgs>),
    /// Generate setup keys
    #[command(alias = "sk")]
    SetupKeys(SetupKeysArgs),
    /// Run prover
    Run(ProverRunArgs),
    /// Initialize bellman-cuda
    #[command(alias = "cuda")]
    InitBellmanCuda(Box<InitBellmanCudaArgs>),
    /// Download compressor keys
    #[command(alias = "ck")]
    CompressorKeys(CompressorKeysArgs),
}

pub(crate) async fn run(shell: &Shell, args: ProverCommands) -> anyhow::Result<()> {
    match args {
        ProverCommands::Init(args) => init::run(*args, shell).await,
        ProverCommands::SetupKeys(args) => setup_keys::run(args, shell).await,
        ProverCommands::Run(args) => run::run(args, shell).await,
        ProverCommands::InitBellmanCuda(args) => init_bellman_cuda::run(shell, *args).await,
        ProverCommands::CompressorKeys(args) => compressor_keys::run(shell, args).await,
    }
}
