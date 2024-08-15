use common::{check_prover_prequisites, cmd::Cmd, logger, spinner::Spinner};
use config::{is_prover_only_system, EcosystemConfig, GeneralProverConfig};
use xshell::{cmd, Shell};

use super::utils::get_link_to_prover;
use crate::messages::{MSG_GENERATING_SK_SPINNER, MSG_SK_GENERATED};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    check_prover_prequisites(shell);

    let link_to_code = if is_prover_only_system(shell)? {
        let general_prover_config = GeneralProverConfig::from_file(shell)?;
        general_prover_config.link_to_code
    } else {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        ecosystem_config.link_to_code
    };

    let link_to_prover = get_link_to_prover(link_to_code);
    shell.change_dir(&link_to_prover);

    let spinner = Spinner::new(MSG_GENERATING_SK_SPINNER);
    let cmd = Cmd::new(cmd!(
        shell,
        "cargo run --features gpu --release --bin key_generator -- 
            generate-sk all --recompute-if-missing 
            --setup-path=vk_setup_data_generator_server_fri/data 
            --path={link_to_prover}/vk_setup_data_generator_server_fri/data"
    ));
    cmd.run()?;
    spinner.finish();
    logger::outro(MSG_SK_GENERATED);

    Ok(())
}
