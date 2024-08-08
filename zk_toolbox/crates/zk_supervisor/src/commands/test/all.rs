use std::thread;

use common::{cmd::Cmd, config::global_config, logger};
use xshell::{cmd, Shell};

use super::{
    args::{
        all::AllArgs, integration::IntegrationArgs, recovery::RecoveryArgs, revert::RevertArgs,
    },
    integration, recovery, revert, upgrade,
};

pub fn run(shell: &Shell, args: AllArgs) -> anyhow::Result<()> {
    let chain = global_config().chain_name.clone();

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
    let _ = integration::run(
        shell,
        IntegrationArgs {
            external_node: false,
        },
    );

    logger::info("Create external node config");
    let mut cmd = cmd!(shell, "zk_inception external-node configs")
        .arg("--db-url")
        .arg(args.db_url.to_string())
        .arg("--db-name")
        .arg(args.db_name)
        .arg("--l1-rpc-url")
        .arg(args.l1_rpc_url);

    if let Some(chain) = chain.clone() {
        cmd = cmd.arg("--chain").arg(chain);
    }

    let _out = Cmd::new(cmd).run();

    logger::info("Init external node");
    let mut cmd = cmd!(shell, "zk_inception external-node init").arg("--ignore-prerequisites");

    if let Some(chain) = chain {
        cmd = cmd.arg("--chain").arg(chain);
    }

    let _out = Cmd::new(cmd).run();

    logger::info("Run recovery tests (from snapshot)");
    let _ = recovery::run(shell, RecoveryArgs { snapshot: true });

    logger::info("Run recovery tests (from genesis)");
    let _ = recovery::run(shell, RecoveryArgs { snapshot: false });

    logger::info("Run server again");
    let _handle = thread::spawn(move || {
        let chain = global_config().chain_name.clone();

        let server_shell = Shell::new().unwrap();
        let mut cmd = cmd!(server_shell, "zk_inception server").arg("--ignore-prerequisites");

        if let Some(chain) = chain {
            cmd = cmd.arg("--chain").arg(chain);
        }

        let _out = Cmd::new(cmd).run_with_output().unwrap();
    });

    logger::info("Run integration tests (external node)");
    let _ = integration::run(
        shell,
        IntegrationArgs {
            external_node: true,
        },
    );

    logger::info("Run revert tests");
    let _ = revert::run(
        shell,
        RevertArgs {
            enable_consensus: false,
            external_node: false,
        },
    );

    logger::info("Run revert tests (external node)");
    let _ = revert::run(
        shell,
        RevertArgs {
            enable_consensus: false,
            external_node: true,
        },
    );

    logger::info("Run upgrade test");
    let _ = upgrade::run(shell);

    Ok(())
}
