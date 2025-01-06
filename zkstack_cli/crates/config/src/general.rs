use std::path::{Path, PathBuf};

use anyhow::Context;
use common::yaml::{merge_yaml, ConfigPatch};
use url::Url;
use xshell::Shell;
pub use zksync_config::configs::GeneralConfig;

use crate::{
    consts::GENERAL_FILE,
    traits::{ConfigWithL2RpcUrl, FileConfigWithDefaultName, ReadConfig},
    ChainConfig,
};

pub struct RocksDbs {
    pub state_keeper: PathBuf,
    pub merkle_tree: PathBuf,
    pub protective_reads: PathBuf,
    pub basic_witness_input_producer: PathBuf,
}

pub struct FileArtifacts {
    pub public_object_store: PathBuf,
    pub prover_object_store: PathBuf,
    pub snapshot: PathBuf,
    pub core_object_store: PathBuf,
}

impl FileArtifacts {
    /// Currently all artifacts are stored in one path, but we keep an opportunity to update this paths
    pub fn new(path: PathBuf) -> Self {
        Self {
            public_object_store: path.clone(),
            prover_object_store: path.clone(),
            snapshot: path.clone(),
            core_object_store: path.clone(),
        }
    }
}

pub fn set_rocks_db_config(config: &mut ConfigPatch, rocks_dbs: RocksDbs) -> anyhow::Result<()> {
    config.insert_path("db.state_keeper_db_path", &rocks_dbs.state_keeper)?;
    config.insert_path("db.merkle_tree.path", &rocks_dbs.merkle_tree)?;
    config.insert_path(
        "protective_reads_writer.db_path",
        &rocks_dbs.protective_reads,
    )?;
    config.insert_path(
        "basic_witness_input_producer.db_path",
        &rocks_dbs.basic_witness_input_producer,
    )?;
    Ok(())
}

pub fn set_file_artifacts(
    config: &mut ConfigPatch,
    file_artifacts: FileArtifacts,
) -> anyhow::Result<()> {
    // FIXME: won't work w/o migrating object store configs
    config.insert_path(
        "prover.prover_object_store.file_backed_base_path",
        &file_artifacts.prover_object_store,
    )?;
    config.insert_path(
        "prover.public_object_store.file_backed_base_path",
        &file_artifacts.public_object_store,
    )?;
    config.insert_path(
        "snapshot_creator.object_store.file_backed_base_path",
        &file_artifacts.snapshot,
    )?;
    config.insert_path(
        "snapshot_recovery.object_store.file_backed_base_path",
        &file_artifacts.snapshot,
    )?;
    config.insert_path(
        "core_object_store.file_backed_base_path",
        &file_artifacts.core_object_store,
    )?;
    Ok(())
}

pub fn override_config(shell: &Shell, path: PathBuf, chain: &ChainConfig) -> anyhow::Result<()> {
    let chain_config_path = chain.path_to_general_config();
    let override_config = serde_yaml::from_str(&shell.read_file(path)?)?;
    let mut chain_config = serde_yaml::from_str(&shell.read_file(chain_config_path.clone())?)?;
    merge_yaml(&mut chain_config, override_config, true)?;
    shell.write_file(chain_config_path, serde_yaml::to_string(&chain_config)?)?;
    Ok(())
}

impl FileConfigWithDefaultName for GeneralConfig {
    const FILE_NAME: &'static str = GENERAL_FILE;
}

impl ReadConfig for GeneralConfig {
    fn read(_shell: &Shell, _path: impl AsRef<Path>) -> anyhow::Result<Self> {
        todo!("remove")
    }
}

impl ConfigWithL2RpcUrl for GeneralConfig {
    fn get_l2_rpc_url(&self) -> anyhow::Result<Url> {
        self.api_config
            .as_ref()
            .map(|api_config| &api_config.web3_json_rpc.http_url)
            .context("API config is missing")?
            .parse()
            .context("Failed to parse L2 RPC URL")
    }
}
