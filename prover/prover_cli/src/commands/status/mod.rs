use clap::Subcommand;

use crate::errors::CLIErrors;

pub(crate) mod l1;

#[derive(Subcommand)]
pub enum StatusCommand {
    L1,
}

impl StatusCommand {
    pub(crate) async fn run(self) -> Result<(), CLIErrors> {
        match self {
            StatusCommand::L1 => l1::run().await,
        }
    }
}
