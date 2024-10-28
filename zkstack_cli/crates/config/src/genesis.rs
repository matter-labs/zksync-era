use std::path::Path;

use anyhow::Context as _;
use xshell::Shell;
use zksync_basic_types::L1ChainId;
pub use zksync_config::GenesisConfig;
use zksync_protobuf_config::{encode_yaml_repr, read_yaml_repr};

use crate::{
    consts::GENESIS_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
    ChainConfig,
};

pub fn update_from_chain_config(
    genesis: &mut GenesisConfig,
    config: &ChainConfig,
) -> anyhow::Result<()> {
    genesis.l2_chain_id = config.chain_id;
    // TODO(EVM-676): for now, the settlement layer is always the same as the L1 network
    genesis.l1_chain_id = L1ChainId(config.l1_network.chain_id());
    genesis.l1_batch_commit_data_generator_mode = config.l1_batch_commit_data_generator_mode;
    genesis.evm_emulator_hash = if config.evm_emulator {
        Some(genesis.evm_emulator_hash.context(
            "impossible to initialize a chain with EVM emulator: the template genesis config \
             does not contain EVM emulator hash",
        )?)
    } else {
        None
    };
    Ok(())
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
        read_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&path, false)
    }
}
