use std::path::Path;

use xshell::Shell;
pub use zksync_config::configs::en_config::ENConfig;

use crate::{
    consts::EN_CONFIG_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig},
};

impl FileConfigWithDefaultName for ENConfig {
    const FILE_NAME: &'static str = EN_CONFIG_FILE;
}

impl ReadConfig for ENConfig {
    fn read(_shell: &Shell, _path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!("remove")
    }
}
