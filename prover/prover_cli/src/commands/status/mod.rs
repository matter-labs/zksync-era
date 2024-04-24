use clap::Subcommand;

pub(crate) mod batch;
mod utils;

#[derive(Subcommand)]
pub enum StatusCommand {
    Batch(batch::Args),
}

impl StatusCommand {
    pub(crate) async fn run(self) -> anyhow::Result<()> {
        match self {
            StatusCommand::Batch(args) => batch::run(args).await,
        }
    }
}
