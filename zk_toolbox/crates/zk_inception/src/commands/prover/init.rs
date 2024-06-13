use anyhow::Ok;
use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{MSG_GENERATING_VK_SPINNER, MSG_INITIALIZING_PROVER, MSG_PROVER_INITIALIZED};

use super::args::init::InitArgs;

pub(crate) async fn run(_args: InitArgs, shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_INITIALIZING_PROVER);
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let mut link_to_prover = ecosystem_config.link_to_code.into_os_string();
    link_to_prover.push("/prover");
    shell.change_dir(link_to_prover.clone());

    let spinner = Spinner::new(MSG_GENERATING_VK_SPINNER);
    let mut cmd = Cmd::new(cmd!(
        shell,
        "cargo run --features gpu --release --bin key_generator -- 
            generate-sk all --recompute-if-missing 
            --setup-path=vk_setup_data_generator_server_fri/data 
            --path={link_to_prover}/vk_setup_data_generator_server_fri/data"
    ));
    cmd.run()?;
    spinner.finish();
    logger::outro(MSG_PROVER_INITIALIZED);

    Ok(())
}
