use clap::{command, Args, Parser, Subcommand};
use zksync_types::url::SensitiveUrl;

use crate::commands::{
    config, debug_proof, delete, get_file_info, requeue, restart, stats, status::StatusCommand,
};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "prover-cli", version = VERSION_STRING, about, long_about = None)]
struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
    #[clap(flatten)]
    config: ProverCLIConfig,
}

// Note: this is set via the `config` command. Values are taken from the file pointed
// by the env var `PLI__CONFIG` or from `$ZKSYNC_HOME/etc/pliconfig` if unset.
#[derive(Args)]
pub struct ProverCLIConfig {
    #[clap(
        default_value = "postgres://postgres:notsecurepassword@localhost/prover_local",
        env("PLI__DB_URL")
    )]
    pub db_url: SensitiveUrl,
}

#[derive(Subcommand)]
enum ProverCommand {
    DebugProof(debug_proof::Args),
    FileInfo(get_file_info::Args),
    Config(ProverCLIConfig),
    Delete(delete::Args),
    #[command(subcommand)]
    Status(StatusCommand),
    Requeue(requeue::Args),
    Restart(restart::Args),
    #[command(about = "Displays L1 Batch proving stats for a given period")]
    Stats(stats::Options),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command, config } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::Config(cfg) => config::run(cfg).await?,
        ProverCommand::Delete(args) => delete::run(args, config).await?,
        ProverCommand::Status(cmd) => cmd.run(config).await?,
        ProverCommand::Requeue(args) => requeue::run(args, config).await?,
        ProverCommand::Restart(args) => restart::run(args).await?,
        ProverCommand::DebugProof(args) => debug_proof::run(args).await?,
        ProverCommand::Stats(args) => stats::run(args, config).await?,
    };

    Ok(())
}
