use clap::{command, Args, Parser, Subcommand};
use zksync_types::url::SensitiveUrl;

use crate::commands::{self, delete, get_file_info, requeue, restart};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name="prover-cli", version=VERSION_STRING, about, long_about = None)]
struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
    #[clap(flatten)]
    config: ProverCLIConfig,
}

// Note: This is a temporary solution for the configuration of the CLI. In the
// future, we should have an `config` command to set the configuration in a
// `.config` file.
#[derive(Args)]
pub struct ProverCLIConfig {
    #[clap(
        long,
        default_value = "postgres://postgres:notsecurepassword@localhost/prover_local"
    )]
    pub db_url: SensitiveUrl,
}

#[derive(Subcommand)]
enum ProverCommand {
    FileInfo(get_file_info::Args),
    Delete(delete::Args),
    #[command(subcommand)]
    Status(commands::StatusCommand),
    Requeue(requeue::Args),
    Restart(restart::Args),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command, config } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::Delete(args) => delete::run(args).await?,
        ProverCommand::Status(cmd) => cmd.run(config).await?,
        ProverCommand::Requeue(args) => requeue::run(args, config).await?,
        ProverCommand::Restart(args) => restart::run(args).await?,
    };

    Ok(())
}
