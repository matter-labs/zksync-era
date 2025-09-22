use anyhow::Context;
use clap::Subcommand;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::ZkStackConfig;

use crate::commands::dev::messages::MSG_RUNNING_SNAPSHOT_CREATOR;

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
    let chain = ZkStackConfig::current_chain(shell)?;

    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    logger::info(MSG_RUNNING_SNAPSHOT_CREATOR);

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --manifest-path ./core/Cargo.toml --bin snapshots_creator --release -- --config-path={config_path} --secrets-path={secrets_path}"))
        .env("RUST_LOG", "snapshots_creator=debug");

    cmd = cmd.with_force_run();
    cmd.run().context("Snapshot")
}
