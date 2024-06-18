use common::logger;
use config::EcosystemConfig;
use xshell::Shell;

use super::utils::get_link_to_prover;

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(&link_to_prover);

    logger::info("Running prover");

    Ok(())
}
