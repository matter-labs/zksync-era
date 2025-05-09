use clap::{command, Args, Parser, Subcommand};
use zksync_types::url::SensitiveUrl;

use crate::commands::{
    config, debug_proof, get_file_info, insert_batch, insert_version, requeue, stats,
    status::StatusCommand,
};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "prover-cli", version = VERSION_STRING, about, long_about = None)]
pub struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
    #[clap(flatten)]
    config: ProverCLIConfig,
}

impl ProverCLI {
    pub async fn start(self) -> anyhow::Result<()> {
        match self.command {
            ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
            ProverCommand::Config(cfg) => config::run(cfg).await?,
            ProverCommand::Status(cmd) => cmd.run(self.config).await?,
            ProverCommand::Requeue(args) => requeue::run(args, self.config).await?,
            ProverCommand::DebugProof(args) => debug_proof::run(args).await?,
            ProverCommand::Stats(args) => stats::run(args, self.config).await?,
            ProverCommand::InsertVersion(args) => insert_version::run(args, self.config).await?,
            ProverCommand::InsertBatch(args) => insert_batch::run(args, self.config).await?,
        };
        Ok(())
    }
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
    #[clap(default_value = "10")]
    pub max_failure_attempts: u32,
}

#[derive(Subcommand)]
pub enum ProverCommand {
    DebugProof(debug_proof::Args),
    FileInfo(get_file_info::Args),
    Config(ProverCLIConfig),
    #[command(subcommand)]
    Status(StatusCommand),
    Requeue(requeue::Args),
    #[command(about = "Displays L1 Batch proving stats for a given period")]
    Stats(stats::Options),
    InsertVersion(insert_version::Args),
    InsertBatch(insert_batch::Args),
}
