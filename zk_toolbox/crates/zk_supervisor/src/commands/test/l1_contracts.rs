use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_L1_CONTRACTS_TEST_SPINNER, MSG_L1_CONTRACTS_TEST_SUCCESS};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);

    let spinner = Spinner::new(MSG_L1_CONTRACTS_TEST_SPINNER);
    Cmd::new(cmd!(shell, "yarn l1-contracts test")).run()?;
    spinner.finish();

    logger::outro(MSG_L1_CONTRACTS_TEST_SUCCESS);

    Ok(())
}
