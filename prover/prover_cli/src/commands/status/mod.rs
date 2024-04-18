use clap::Subcommand;

use crate::errors::CLIErrors;

pub(crate) mod batch;
pub(crate) mod l1;

#[derive(Subcommand)]
pub enum StatusCommand {
    Batch(batch::Args),
    L1,
}

impl StatusCommand {
    pub(crate) async fn run(self) -> Result<(), CLIErrors> {
        match self {
            // TODO: Improve error handeling for batch
            StatusCommand::Batch(args) => batch::run(args).await.map_err(CLIErrors::AnyHowError),
            StatusCommand::L1 => l1::run().await,
        }
    }
}
