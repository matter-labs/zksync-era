use std::path::Path;

use xshell::Shell;

use crate::consts::{CONFIGS_PATH, WALLETS_FILE};

pub fn copy_configs(shell: &Shell, link_to_code: &Path, target_path: &Path) -> anyhow::Result<()> {
    let original_configs = link_to_code.join(CONFIGS_PATH);
    for file in shell.read_dir(original_configs)? {
        if file.is_dir() {
            continue;
        }

        if let Some(name) = file.file_name() {
            // Do not copy wallets file
            if name != WALLETS_FILE {
                shell.copy_file(file, target_path)?;
            }
        }
    }
    Ok(())
}
