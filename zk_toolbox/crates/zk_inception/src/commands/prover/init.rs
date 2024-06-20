use xshell::Shell;

use super::args::init::ProverInitArgs;

pub(crate) async fn run(_args: ProverInitArgs, _shell: &Shell) -> anyhow::Result<()> {
    Ok(())
}
