use zksync_config::configs::consensus::ConsensusConfig;
use zksync_protobuf_config::encode_yaml_repr;

use crate::{
    traits::{FileConfigWithDefaultName, SaveConfig},
    CONSENSUS_CONFIG_FILE,
};

impl FileConfigWithDefaultName for ConsensusConfig {
    const FILE_NAME: &'static str = CONSENSUS_CONFIG_FILE;
}

impl SaveConfig for ConsensusConfig {
    fn save(&self, shell: &xshell::Shell, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let bytes = encode_yaml_repr::<zksync_protobuf_config::proto::consensus::Config>(self)?;
        Ok(shell.write_file(path.as_ref(), bytes)?)
    }
}
