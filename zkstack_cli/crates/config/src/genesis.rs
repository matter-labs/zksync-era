use std::path::Path;

use common::yaml::PatchedConfig;
use xshell::Shell;
pub use zksync_config::GenesisConfig;

use crate::{
    consts::GENESIS_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig},
    ChainConfig,
};

pub fn update_from_chain_config(
    genesis: &mut PatchedConfig,
    config: &ChainConfig,
) -> anyhow::Result<()> {
    genesis.insert("l2_chain_id", config.chain_id.as_u64());
    // TODO(EVM-676): for now, the settlement layer is always the same as the L1 network
    genesis.insert("l1_chain_id", config.l1_network.chain_id());
    genesis.insert_yaml(
        "l1_batch_commit_data_generator_mode",
        config.l1_batch_commit_data_generator_mode,
    );
    // FIXME: check EVM emulator hash?
    Ok(())
}

impl FileConfigWithDefaultName for GenesisConfig {
    const FILE_NAME: &'static str = GENESIS_FILE;
}

impl ReadConfig for GenesisConfig {
    fn read(_shell: &Shell, _path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!("remove")
    }
}
