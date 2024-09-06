use std::str::FromStr;

use clap::{command, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use zksync_types::url::SensitiveUrl;

use crate::commands::{
    config::{utils::load_config, ConfigCommand},
    debug_proof, delete, get_file_info, requeue, restart, stats,
    status::StatusCommand,
};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_DB_URL: &str = "postgres://postgres:notsecurepassword@localhost/prover_local";

#[derive(Parser)]
#[command(name = "prover-cli", version = VERSION_STRING, about, long_about = None)]
pub struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
}

impl ProverCLI {
    pub async fn start(self) -> anyhow::Result<()> {
        if let ProverCommand::Config(cmd) = self.command {
            return cmd.run().await;
        }
        let cfg = load_config().await?;
        match self.command {
            ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
            ProverCommand::Delete(args) => delete::run(args, cfg).await?,
            ProverCommand::Status(cmd) => cmd.run(cfg).await?,
            ProverCommand::Requeue(args) => requeue::run(args, cfg).await?,
            ProverCommand::Restart(args) => restart::run(args).await?,
            ProverCommand::DebugProof(args) => debug_proof::run(args).await?,
            ProverCommand::Stats(args) => stats::run(args, cfg).await?,
            ProverCommand::Config(_) => unreachable!(),
        };
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
pub struct ProverCLIConfig {
    pub db_url: Option<String>,
}

impl ProverCLIConfig {
    pub fn get_db_url(&self) -> SensitiveUrl {
        match &self.db_url {
            Some(url_str) => SensitiveUrl::from_str(url_str).expect("Failed to parse URL"),
            None => SensitiveUrl::from_str(DEFAULT_DB_URL).expect("Failed to parse default URL"),
        }
    }
}

#[derive(Subcommand)]
pub enum ProverCommand {
    DebugProof(debug_proof::Args),
    FileInfo(get_file_info::Args),
    #[command(subcommand)]
    Config(ConfigCommand),
    Delete(delete::Args),
    #[command(subcommand)]
    Status(StatusCommand),
    Requeue(requeue::Args),
    Restart(restart::Args),
    #[command(about = "Displays L1 Batch proving stats for a given period")]
    Stats(stats::Options),
}
