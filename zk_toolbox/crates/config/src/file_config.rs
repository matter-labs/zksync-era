use std::path::{Path, PathBuf};

use xshell::Shell;

use crate::consts::LOCAL_CONFIGS_PATH;

pub fn create_local_configs_dir(
    shell: &Shell,
    base_path: impl AsRef<Path>,
) -> xshell::Result<PathBuf> {
    shell.create_dir(base_path.as_ref().join(LOCAL_CONFIGS_PATH))
}
