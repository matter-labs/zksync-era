use std::path::PathBuf;

use config::EcosystemConfig;

pub(crate) fn get_link_to_prover(config: &EcosystemConfig) -> PathBuf {
    let link_to_code = config.link_to_code.clone();
    let mut link_to_prover = link_to_code.into_os_string();
    link_to_prover.push("/prover");
    link_to_prover.into()
}
