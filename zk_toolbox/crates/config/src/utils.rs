use std::path::PathBuf;

use xshell::Shell;

use crate::{EcosystemConfig, GeneralProverConfig};

// Find file in all parents repository and return necessary path or an empty error if nothing has been found
pub(crate) fn find_file(shell: &Shell, path_buf: PathBuf, file_name: &str) -> Result<PathBuf, ()> {
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

pub fn is_prover_only_system(shell: &Shell) -> anyhow::Result<bool> {
    let current_path = shell.current_dir();
    match EcosystemConfig::from_file(shell) {
        Ok(_) => Ok(false),
        Err(_) => {
            let _dir = shell.push_dir(current_path.clone());
            match GeneralProverConfig::from_file(shell) {
                Ok(_) => Ok(true),
                Err(_) => {
                    return Err(anyhow::anyhow!(
                        "There was no ecosystem or prover subsystem config found."
                    ));
                }
            }
        }
    }
}
