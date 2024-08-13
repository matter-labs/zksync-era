use std::thread;

use common::{cmd::Cmd, config::global_config, logger};
use xshell::{cmd, Shell};

use super::{
    args::{
        all::AllArgs, integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs,
        upgrade::UpgradeArgs,
    },
    integration, recovery, revert, upgrade,
};
use crate::messages::MSG_RUNNING_EXTERNAL_NODE;

pub fn run(shell: &Shell, args: AllArgs) -> anyhow::Result<()> {
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
        let chain = global_config().chain_name.clone();

        let mut cmd =
            cmd!(en_shell, "zk_inception external-node run").arg("--ignore-prerequisites");

        if let Some(chain) = chain {
            cmd = cmd.arg("--chain").arg(chain);
        }

        let _out = Cmd::new(cmd).run_with_output().unwrap();
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
