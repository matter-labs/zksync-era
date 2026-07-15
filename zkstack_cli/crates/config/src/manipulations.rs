use std::path::Path;

use xshell::Shell;

use crate::consts::WALLETS_FILE;

pub fn copy_configs(
    shell: &Shell,
    default_configs: &Path,
    target_path: &Path,
) -> anyhow::Result<()> {
    for file in shell.read_dir(default_configs)? {
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
