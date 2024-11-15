use std::path::{Path, PathBuf};

use types::L1Network;
use xshell::Shell;

use crate::ECOSYSTEM_PATH;

// Find file in all parents repository and return necessary path or an empty error if nothing has been found
pub fn find_file(shell: &Shell, path_buf: PathBuf, file_name: &str) -> Result<PathBuf, ()> {
    let _dir = shell.push_dir(path_buf);
    if shell.path_exists(file_name) {
        Ok(shell.current_dir())
    } else {
        let current_dir = shell.current_dir();
        let Some(path) = current_dir.parent() else {
            return Err(());
        };
        find_file(shell, path.to_path_buf(), file_name)
    }
}

pub fn get_preexisting_ecosystem_contracts_path(
    link_to_code: &Path,
    l1_network: L1Network,
) -> PathBuf {
    link_to_code
        .join(ECOSYSTEM_PATH)
        .join(format!("{}.yaml", l1_network.to_string().to_lowercase()))
}
