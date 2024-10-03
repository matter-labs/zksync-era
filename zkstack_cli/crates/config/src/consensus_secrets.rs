use std::path::Path;

use xshell::Shell;
use zksync_config::configs::consensus::ConsensusSecrets;
use zksync_protobuf_config::decode_yaml_repr;

use crate::traits::ReadConfig;

impl ReadConfig for ConsensusSecrets {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        decode_yaml_repr::<zksync_protobuf_config::proto::secrets::ConsensusSecrets>(&path, false)
    }
}
