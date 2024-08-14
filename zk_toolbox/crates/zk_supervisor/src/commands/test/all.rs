use std::{thread, vec};

use anyhow::Context;
use common::{config::global_config, logger};
use config::EcosystemConfig;
use xshell::Shell;

use super::{
    args::{
        all::AllArgs, integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs,
        upgrade::UpgradeArgs,
    },
    integration, recovery, revert, upgrade,
};
use crate::messages::{MSG_CHAIN_NOT_FOUND_ERR, MSG_RUNNING_EXTERNAL_NODE};

pub fn run(shell: &Shell, args: AllArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)
        .unwrap();

    integration::run(
        shell,
        IntegrationArgs {
            external_node: false,
            no_deps: args.no_deps,
        },
    )?;

    recovery::run(
        shell,
        RecoveryArgs {
            snapshot: true,
            no_deps: args.no_deps,
        },
    )?;

    recovery::run(
        shell,
        RecoveryArgs {
            snapshot: false,
            no_deps: args.no_deps,
        },
    )?;

    logger::info(MSG_RUNNING_EXTERNAL_NODE);
    let en_shell = shell.clone();
    let _handle = thread::spawn(move || {
        let config_path = chain.path_to_general_config();
        let secrets_path = chain.path_to_secrets_config();
        let en_config_path = chain.path_to_external_node_config();

        common::external_node::run(
            &en_shell,
            config_path.to_str().unwrap(),
            secrets_path.to_str().unwrap(),
            en_config_path.to_str().unwrap(),
            vec![],
        )
        .context("Failed to run external node")
        .unwrap();
    });

    integration::run(
        shell,
        IntegrationArgs {
            external_node: true,
            no_deps: args.no_deps,
        },
    )?;

    revert::run(
        shell,
        RevertArgs {
            enable_consensus: false,
            external_node: true,
            no_deps: args.no_deps,
        },
    )?;

    upgrade::run(
        shell,
        UpgradeArgs {
            no_deps: args.no_deps,
        },
    )?;

    Ok(())
}
