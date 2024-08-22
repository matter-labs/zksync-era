use anyhow::Context;
use clap::Subcommand;
use common::{cmd::Cmd, config::global_config, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_RUNNING_SNAPSHOT_CREATOR};

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
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_chain(
            global_config()
                .chain_name
                .clone()
                .or(Some(ecosystem.default_chain.clone())),
        )
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let config_path = chain.path_to_general_config();
    let secrets_path = chain.path_to_secrets_config();

    logger::info(MSG_RUNNING_SNAPSHOT_CREATOR);

    let mut cmd = Cmd::new(cmd!(shell, "cargo run --bin snapshots_creator --release -- --config-path={config_path} --secrets-path={secrets_path}"))
        .env("RUST_LOG", "snapshots_creator=debug");

    cmd = cmd.with_force_run();
    cmd.run().context("Snapshot")
}
