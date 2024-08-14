use std::path::Path;

use xshell::Shell;

use crate::{
    consts::{CONFIGS_PATH, WALLETS_FILE},
    PROVER_FILE, SECRETS_FILE,
};

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

pub fn copy_prover_configs(
    shell: &Shell,
    link_to_code: &Path,
    target_path: &Path,
) -> anyhow::Result<()> {
    let prover_config = link_to_code.join(CONFIGS_PATH).join(PROVER_FILE);
    let secrets_config = link_to_code.join(CONFIGS_PATH).join(SECRETS_FILE);
    shell.copy_file(prover_config, target_path)?;
    shell.copy_file(secrets_config, target_path)?;
    Ok(())
}
