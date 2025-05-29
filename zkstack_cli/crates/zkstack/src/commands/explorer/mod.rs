use clap::Subcommand;
use init::ExplorerInitArgs;
use xshell::Shell;

mod backend;
mod init;
mod run;

#[derive(Subcommand, Debug)]
pub enum ExplorerCommands {
    /// Initialize explorer (create database to store explorer data and generate docker
    /// compose file with explorer services). Runs for all chains, unless --chain is passed
    Init(ExplorerInitArgs),
    /// Start explorer backend services (api, data_fetcher, worker) for a given chain.
    /// Uses default chain, unless --chain is passed
    #[command(alias = "backend")]
    RunBackend,
    /// Run explorer app
    Run,
}

pub(crate) async fn run(shell: &Shell, args: ExplorerCommands) -> anyhow::Result<()> {
    match args {
        ExplorerCommands::Init(args) => init::run(args, shell).await,
        ExplorerCommands::Run => run::run(shell),
        ExplorerCommands::RunBackend => backend::run(shell),
    }
}
