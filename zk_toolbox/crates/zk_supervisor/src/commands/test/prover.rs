use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::{defaults::TEST_DATABASE_PROVER_URL, messages::MSG_PROVER_TEST_SUCCESS};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let _dir_guard = shell.push_dir(ecosystem.link_to_code.join("prover"));

    Cmd::new(cmd!(shell, "cargo test --release --workspace --locked"))
        .with_force_run()
        .env("TEST_DATABASE_PROVER_URL", TEST_DATABASE_PROVER_URL)
        .run()?;

    logger::outro(MSG_PROVER_TEST_SUCCESS);
    Ok(())
}
