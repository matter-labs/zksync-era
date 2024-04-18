use clap::Subcommand;

pub(crate) mod batch;
pub(crate) mod l1;

#[derive(Subcommand)]
pub enum StatusCommand {
    Batch(batch::Args),
    L1,
}

impl StatusCommand {
    pub(crate) async fn run(self) -> anyhow::Result<()> {
        match self {
            StatusCommand::Batch(args) => batch::run(args).await,
            StatusCommand::L1 => l1::run().await,
        }
    }
}
