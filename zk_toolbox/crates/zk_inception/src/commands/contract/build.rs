use common::{cmd::Cmd, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::MSG_CONTRACT_BUILDING;

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;

    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);
    let spinner = Spinner::new(MSG_CONTRACT_BUILDING);

    Cmd::new(cmd!(shell, "yarn l1-contracts build")).run()?;
    Cmd::new(cmd!(shell, "yarn l2-contracts build")).run()?;

    spinner.finish();
    Ok(())
}
