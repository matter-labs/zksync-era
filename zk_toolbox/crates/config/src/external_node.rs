use std::path::Path;

use xshell::Shell;
pub use zksync_config::configs::en_config::ENConfig;
use zksync_protobuf_config::{decode_yaml_repr, encode_yaml_repr};

use crate::{
    consts::EN_CONFIG_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
};

impl FileConfigWithDefaultName for ENConfig {
    const FILE_NAME: &'static str = EN_CONFIG_FILE;
}

impl SaveConfig for ENConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let bytes = encode_yaml_repr::<zksync_protobuf_config::proto::en::ExternalNode>(self)?;
        Ok(shell.write_file(path, bytes)?)
    }
}

impl ReadConfig for ENConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        decode_yaml_repr::<zksync_protobuf_config::proto::en::ExternalNode>(&path, false)
    }
}
