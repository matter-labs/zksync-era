use common::{
    git::{pull, submodule_update},
    logger,
    spinner::Spinner,
};
use config::EcosystemConfig;
use xshell::Shell;

use crate::messages::{
    MSG_PULLING_ZKSYNC_CODE_SPINNER, MSG_UPDATING_SUBMODULES_SPINNER, MSG_UPDATING_ZKSYNC,
    MSG_ZKSYNC_UPDATED,
};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_UPDATING_ZKSYNC);
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let link_to_code = ecosystem.link_to_code;
    let spinner = Spinner::new(MSG_PULLING_ZKSYNC_CODE_SPINNER);
    pull(shell, link_to_code.clone())?;
    spinner.finish();
    let spinner = Spinner::new(MSG_UPDATING_SUBMODULES_SPINNER);
    submodule_update(shell, link_to_code.clone())?;
    spinner.finish();

    logger::outro(MSG_ZKSYNC_UPDATED);
    Ok(())
}
