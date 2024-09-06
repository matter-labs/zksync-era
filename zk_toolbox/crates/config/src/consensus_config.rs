use zksync_config::configs::consensus::ConsensusConfig;

use crate::{traits::FileConfigWithDefaultName, CONSENSUS_CONFIG_FILE};

impl FileConfigWithDefaultName for ConsensusConfig {
    const FILE_NAME: &'static str = CONSENSUS_CONFIG_FILE;
}
