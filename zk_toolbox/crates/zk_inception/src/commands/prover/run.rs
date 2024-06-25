use common::logger;
use config::EcosystemConfig;
use xshell::Shell;

use super::{args::run::ProverRunArgs, utils::get_link_to_prover};

pub(crate) async fn run(args: ProverRunArgs, shell: &Shell) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt()?;
    logger::debug(format!("Prover args: {:?}", args));
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let link_to_prover = get_link_to_prover(&ecosystem_config);
    shell.change_dir(&link_to_prover);

    logger::info("Running prover");

    Ok(())
}
