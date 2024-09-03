use clap::Subcommand;
use xshell::Shell;

mod backend;
mod init;
mod run;

#[derive(Subcommand, Debug)]
pub enum ExplorerCommands {
    /// Initialize explorer
    Init,
    /// Start explorer backend services (api, data_fetcher, worker)
    #[command(alias = "backend")]
    RunBackend,
    /// Run explorer app
    Run,
}

pub(crate) async fn run(shell: &Shell, args: ExplorerCommands) -> anyhow::Result<()> {
    match args {
        ExplorerCommands::Init => init::run(shell).await,
        ExplorerCommands::Run => run::run(shell),
        ExplorerCommands::RunBackend => backend::run(shell),
    }
}
