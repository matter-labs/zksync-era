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

pub fn run(shell: &Shell, args: AllArgs) -> anyhow::Result<()> {
    logger::info("Run server");
    let _handle = thread::spawn(move || {
        let chain = global_config().chain_name.clone();

        let server_shell = Shell::new().unwrap();
        let mut cmd = cmd!(server_shell, "zk_inception server").arg("--ignore-prerequisites");

        if let Some(chain) = chain {
            cmd = cmd.arg("--chain").arg(chain);
        }

        let _out = Cmd::new(cmd).run_with_output().unwrap();
    });

    logger::info("Run integration tests");
    integration::run(
        shell,
        IntegrationArgs {
            external_node: false,
            no_deps: args.no_deps,
        },
    )?;

    logger::info("Run recovery tests (from snapshot)");
    recovery::run(
        shell,
        RecoveryArgs {
            snapshot: true,
            no_deps: args.no_deps,
        },
    )?;

    logger::info("Run recovery tests (from genesis)");
    recovery::run(
        shell,
        RecoveryArgs {
            snapshot: false,
            no_deps: args.no_deps,
        },
    )?;

    logger::info("Run external-node");
    let _handle = thread::spawn(move || {
        let chain = global_config().chain_name.clone();

        let server_shell = Shell::new().unwrap();
        let mut cmd =
            cmd!(server_shell, "zk_inception external-node run").arg("--ignore-prerequisites");

        if let Some(chain) = chain {
            cmd = cmd.arg("--chain").arg(chain);
        }

        let _out = Cmd::new(cmd).run_with_output().unwrap();
    });

    logger::info("Run integration tests (external node)");
    integration::run(
        shell,
        IntegrationArgs {
            external_node: true,
            no_deps: args.no_deps,
        },
    )?;

    logger::info("Run revert tests");
    revert::run(
        shell,
        RevertArgs {
            enable_consensus: false,
            external_node: false,
            no_deps: args.no_deps,
        },
    )?;

    logger::info("Run revert tests (external node)");
    revert::run(
        shell,
        RevertArgs {
            enable_consensus: false,
            external_node: true,
            no_deps: args.no_deps,
        },
    )?;

    logger::info("Run upgrade test");
    upgrade::run(
        shell,
        UpgradeArgs {
            no_deps: args.no_deps,
        },
    )?;

    Ok(())
}
