use anyhow::Ok;
use common::logger;
use xshell::Shell;

use super::args::init::InitArgs;

pub(crate) async fn run(_args: InitArgs, _shell: &Shell) -> anyhow::Result<()> {
    logger::debug("Initializing prover");
    Ok(())
}
