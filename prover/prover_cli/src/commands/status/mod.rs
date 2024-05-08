use clap::Subcommand;

use crate::cli::ProverCLIConfig;

pub(crate) mod batch;
mod utils;

#[derive(Subcommand)]
pub enum StatusCommand {
    Batch(batch::Args),
}

impl StatusCommand {
    pub(crate) async fn run(self, config: ProverCLIConfig) -> anyhow::Result<()> {
        match self {
            StatusCommand::Batch(args) => batch::run(args, config).await,
        }
    }
}
