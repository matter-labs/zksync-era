use super::StatusCommand;

pub(crate) mod jobs;

pub(crate) async fn run(status_cmd: StatusCommand) -> anyhow::Result<()> {
    match status_cmd {
        StatusCommand::Jobs(args) => jobs::run(args).await,
    }
}
