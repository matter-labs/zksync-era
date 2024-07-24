use anyhow::Context;
use clap::Subcommand;
use common::{cmd::Cmd, logger};
use xshell::{cmd, Shell};

#[derive(Subcommand, Debug)]
pub enum SnapshotCommands {
    Create,
}

pub(crate) async fn run(shell: &Shell, args: SnapshotCommands) -> anyhow::Result<()> {
    match args {
        SnapshotCommands::Create => {
            create(shell).await?;
        }
    }

    Ok(())
}

async fn create(shell: &Shell) -> anyhow::Result<()> {
    logger::info("Log");

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --bin snapshots_creator --release"))
        .env("RUST_LOG", "snapshots_creator=debug");

    cmd = cmd.with_force_run();
    cmd.run().context("MSG")
}
