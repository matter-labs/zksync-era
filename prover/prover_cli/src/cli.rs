use clap::{command, Args, Parser, Subcommand};
use zksync_types::url::SensitiveUrl;

use crate::commands::{self, get_file_info};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name="prover-cli", version=VERSION_STRING, about, long_about = None)]
struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
    #[clap(flatten)]
    config: ProverCLIConfig,
}

#[derive(Args)]
pub struct ProverCLIConfig {
    #[clap(
        long,
        default_value = "postgres://postgres:notsecurepassword@localhost/prover_local"
    )]
    pub db_url: Option<SensitiveUrl>,
}

#[derive(Subcommand)]
enum ProverCommand {
    FileInfo(get_file_info::Args),
    #[command(subcommand)]
    Status(commands::StatusCommand),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command, config } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::Status(cmd) => cmd.run(config).await?,
    };

    Ok(())
}
