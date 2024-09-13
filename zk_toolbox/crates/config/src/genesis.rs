use std::path::Path;

use xshell::Shell;
use zksync_basic_types::L1ChainId;
pub use zksync_config::GenesisConfig;
use zksync_protobuf_config::{decode_yaml_repr, encode_yaml_repr};

use crate::{
    consts::GENESIS_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
    ChainConfig,
};

pub fn update_from_chain_config(genesis: &mut GenesisConfig, config: &ChainConfig) {
    genesis.l2_chain_id = config.chain_id;
    // TODO(EVM-676): for now, the settlement layer is always the same as the L1 network
    genesis.l1_chain_id = L1ChainId(config.l1_network.chain_id());
    genesis.l1_batch_commit_data_generator_mode = config.l1_batch_commit_data_generator_mode;
}

impl FileConfigWithDefaultName for GenesisConfig {
    const FILE_NAME: &'static str = GENESIS_FILE;
}

impl SaveConfig for GenesisConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let bytes = encode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(self)?;
        Ok(shell.write_file(path, bytes)?)
    }
}

impl ReadConfig for GenesisConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        decode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&path, false)
    }
}
