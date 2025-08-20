use xshell::Shell;
use zkstack_cli_common::{git, logger, spinner::Spinner};
use zkstack_cli_config::{ERA_OBSERBAVILITY_DIR, ERA_OBSERBAVILITY_GIT_REPO};

use crate::messages::{
    MSG_DOWNLOADING_ERA_OBSERVABILITY_SPINNER, MSG_ERA_OBSERVABILITY_ALREADY_SETUP,
};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let path_to_era_observability = shell.current_dir().join(ERA_OBSERBAVILITY_DIR);
    if shell.path_exists(path_to_era_observability.clone()) {
        logger::info(MSG_ERA_OBSERVABILITY_ALREADY_SETUP);
        return Ok(());
    }

    let spinner = Spinner::new(MSG_DOWNLOADING_ERA_OBSERVABILITY_SPINNER);
    git::clone(
        shell,
        &shell.current_dir(),
        ERA_OBSERBAVILITY_GIT_REPO,
        ERA_OBSERBAVILITY_DIR,
    )?;
    spinner.finish();

    Ok(())
}
