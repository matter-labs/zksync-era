use std::path::PathBuf;

pub(crate) fn get_link_to_prover(link_to_code: PathBuf) -> PathBuf {
    let mut link_to_prover = link_to_code.into_os_string();
    link_to_prover.push("/prover");
    link_to_prover.into()
}
