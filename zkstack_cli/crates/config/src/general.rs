use std::path::{Path, PathBuf};

use anyhow::Context;
use common::yaml::merge_yaml;
use url::Url;
use xshell::Shell;
use zksync_config::configs::object_store::ObjectStoreMode;
pub use zksync_config::configs::GeneralConfig;
use zksync_protobuf_config::{encode_yaml_repr, read_yaml_repr};

use crate::{
    consts::GENERAL_FILE,
    traits::{ConfigWithL2RpcUrl, FileConfigWithDefaultName, ReadConfig, SaveConfig},
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

pub fn set_rocks_db_config(config: &mut GeneralConfig, rocks_dbs: RocksDbs) -> anyhow::Result<()> {
    config
        .db_config
        .as_mut()
        .context("DB config is not presented")?
        .state_keeper_db_path = rocks_dbs.state_keeper.to_str().unwrap().to_string();
    config
        .db_config
        .as_mut()
        .context("DB config is not presented")?
        .merkle_tree
        .path = rocks_dbs.merkle_tree.to_str().unwrap().to_string();
    config
        .protective_reads_writer_config
        .as_mut()
        .context("Protective reads config is not presented")?
        .db_path = rocks_dbs.protective_reads.to_str().unwrap().to_string();
    config
        .basic_witness_input_producer_config
        .as_mut()
        .context("Basic witness input producer config is not presented")?
        .db_path = rocks_dbs
        .basic_witness_input_producer
        .to_str()
        .unwrap()
        .to_string();
    Ok(())
}

pub fn set_file_artifacts(config: &mut GeneralConfig, file_artifacts: FileArtifacts) {
    macro_rules! set_artifact_path {
        ($config:expr, $name:ident, $value:expr) => {
            $config
                .as_mut()
                .map(|a| set_artifact_path!(a.$name, $value))
        };

        ($config:expr, $value:expr) => {
            $config.as_mut().map(|a| {
                if let ObjectStoreMode::FileBacked {
                    ref mut file_backed_base_path,
                } = &mut a.mode
                {
                    *file_backed_base_path = $value.to_str().unwrap().to_string()
                }
            })
        };
    }

    set_artifact_path!(
        config.prover_config,
        prover_object_store,
        file_artifacts.prover_object_store
    );
    set_artifact_path!(
        config.prover_config,
        public_object_store,
        file_artifacts.public_object_store
    );
    set_artifact_path!(
        config.snapshot_creator,
        object_store,
        file_artifacts.snapshot
    );
    set_artifact_path!(
        config.snapshot_recovery,
        object_store,
        file_artifacts.snapshot
    );

    set_artifact_path!(config.core_object_store, file_artifacts.core_object_store);
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

impl SaveConfig for GeneralConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let bytes =
            encode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(self)?;
        Ok(shell.write_file(path, bytes)?)
    }
}

impl ReadConfig for GeneralConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        read_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&path, false)
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
