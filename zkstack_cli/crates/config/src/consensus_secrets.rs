use std::path::Path;

use xshell::Shell;
use zksync_config::configs::consensus::ConsensusSecrets;

use crate::traits::ReadConfig;

impl ReadConfig for ConsensusSecrets {
    fn read(_shell: &Shell, _path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!("remove")
    }
}
