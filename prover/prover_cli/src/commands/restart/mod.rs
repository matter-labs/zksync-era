use clap::Subcommand;

pub(crate) mod batch;
pub(crate) mod job;

#[derive(Subcommand)]
pub enum RestartCommand {
    Batch(batch::Args),
    Job(job::Args),
}

impl RestartCommand {
    pub(crate) async fn run(self) -> anyhow::Result<()> {
        match self {
            RestartCommand::Batch(args) => batch::run(args).await,
            RestartCommand::Job(args) => job::run(args).await,
        }
    }
}
