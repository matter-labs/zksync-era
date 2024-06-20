use xshell::Shell;

use super::args::init::ProverInitArgs;

pub(crate) async fn run(args: ProverInitArgs, _shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();

    Ok(())
}
