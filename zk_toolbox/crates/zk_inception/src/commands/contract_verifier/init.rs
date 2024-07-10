use xshell::Shell;

use super::args::init::InitContractVerifierArgs;

pub(crate) async fn run(shell: &Shell, args: InitContractVerifierArgs) -> anyhow::Result<()> {
    args.fill_values_with_prompt(shell)?;
    Ok(())
}
