use std::path::{Path, PathBuf};

use xshell::Shell;
use zkstack_cli_common::yaml::merge_yaml;

use crate::{
    raw::{PatchedConfig, RawConfig},
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

pub fn set_rocks_db_config(config: &mut PatchedConfig, rocks_dbs: RocksDbs) -> anyhow::Result<()> {
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
    config: &mut PatchedConfig,
    file_artifacts: FileArtifacts,
) -> anyhow::Result<()> {
    set_file_backed_path_if_selected(
        config,
        "prover.prover_object_store",
        &file_artifacts.prover_object_store,
    )?;
    set_file_backed_path_if_selected(
        config,
        "prover.public_object_store",
        &file_artifacts.public_object_store,
    )?;
    set_file_backed_path_if_selected(
        config,
        "snapshot_creator.object_store",
        &file_artifacts.snapshot,
    )?;
    set_file_backed_path_if_selected(
        config,
        "snapshot_recovery.object_store",
        &file_artifacts.snapshot,
    )?;
    Ok(())
}

fn set_file_backed_path_if_selected(
    config: &mut PatchedConfig,
    prefix: &str,
    path: &Path,
) -> anyhow::Result<()> {
    let container = config.base().get_raw(&format!("{prefix}.file_backed"));
    if matches!(container, Some(serde_yaml::Value::Mapping(_))) {
        config.insert_path(&format!("{prefix}.file_backed.file_backed_base_path"), path)?;
    }
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

pub fn get_da_client_type(general: &RawConfig) -> Option<&str> {
    general.get_raw("da_client").and_then(|val| {
        let val = val.as_mapping()?;
        if val.len() == 1 {
            val.keys().next()?.as_str()
        } else {
            None
        }
    })
}
