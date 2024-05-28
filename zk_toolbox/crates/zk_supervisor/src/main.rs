use anyhow::Context;
use common::cmd::Cmd;
use common::config::{init_global_config, GlobalConfig};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

fn main() {
    human_panic::setup_panic!();
    run_integration_tests().unwrap()
}

const TS_INTEGRATION_PATH: &str = "core/tests/ts-integration";
fn run_integration_tests() -> anyhow::Result<()> {
    let shell = Shell::new()?;
    let config = EcosystemConfig::from_file(&shell).context("Era not initialized")?;
    let _dir_guard = shell.push_dir(config.link_to_code.join(TS_INTEGRATION_PATH));
    init_global_config(GlobalConfig {
        verbose: false,
        chain_name: None,
        ignore_prerequisites: false,
    });

    build_test_contracts(&shell)?;
    Cmd::new(
        cmd!(shell, "yarn jest --forceExit --testTimeout 60000")
            .env("CHAIN_NAME", config.default_chain),
    )
    .with_force_run()
    .run()?;

    Ok(())
}

fn build_test_contracts(shell: &Shell) -> anyhow::Result<()> {
    Cmd::new(cmd!(shell, "yarn build")).with_force_run().run()?;
    Cmd::new(cmd!(shell, "yarn build-yul"))
        .with_force_run()
        .run()?;
    Ok(())
}
