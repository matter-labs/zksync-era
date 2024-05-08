use clap::Subcommand;

use crate::cli::ProverCLIConfig;

pub(crate) mod batch;
pub(crate) mod l1;
mod utils;

#[derive(Subcommand)]
pub enum StatusCommand {
    Batch(batch::Args),
    L1,
}

impl StatusCommand {
    pub(crate) async fn run(self, config: ProverCLIConfig) -> anyhow::Result<()> {
        match self {
            StatusCommand::Batch(args) => batch::run(args, config).await,
            StatusCommand::L1 => l1::run().await,
        }
    }
}
