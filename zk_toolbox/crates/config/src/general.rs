use std::path::{Path, PathBuf};

use anyhow::Context;
use url::Url;
use xshell::Shell;
pub use zksync_config::configs::GeneralConfig;
use zksync_protobuf_config::{decode_yaml_repr, encode_yaml_repr};

use crate::{
    consts::GENERAL_FILE,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig},
};

pub struct RocksDbs {
    pub state_keeper: PathBuf,
    pub merkle_tree: PathBuf,
}

pub fn set_rocks_db_config(config: &mut GeneralConfig, rocks_dbs: RocksDbs) -> anyhow::Result<()> {
    config
        .db_config
        .as_mut()
        .context("Db config")?
        .state_keeper_db_path = rocks_dbs.state_keeper.to_str().unwrap().to_string();
    config
        .db_config
        .as_mut()
        .context("Db config")?
        .merkle_tree
        .path = rocks_dbs.merkle_tree.to_str().unwrap().to_string();
    Ok(())
}

pub fn ports_config(config: &GeneralConfig) -> Option<PortsConfig> {
    let api = config.api_config.as_ref()?;
    Some(PortsConfig {
        web3_json_rpc_http_port: api.web3_json_rpc.http_port,
        web3_json_rpc_ws_port: api.web3_json_rpc.ws_port,
        healthcheck_port: api.healthcheck.port,
        merkle_tree_port: api.merkle_tree.port,
        prometheus_listener_port: api.prometheus.listener_port,
    })
}

pub fn update_ports(config: &mut GeneralConfig, ports_config: &PortsConfig) -> anyhow::Result<()> {
    let api = config
        .api_config
        .as_mut()
        .context("Api config is not presented")?;
    api.web3_json_rpc.http_port = ports_config.web3_json_rpc_http_port;
    update_port_in_url(
        &mut api.web3_json_rpc.http_url,
        ports_config.web3_json_rpc_http_port,
    )?;
    api.web3_json_rpc.ws_port = ports_config.web3_json_rpc_ws_port;
    update_port_in_url(
        &mut api.web3_json_rpc.ws_url,
        ports_config.web3_json_rpc_ws_port,
    )?;
    api.healthcheck.port = ports_config.healthcheck_port;
    api.merkle_tree.port = ports_config.merkle_tree_port;
    api.prometheus.listener_port = ports_config.prometheus_listener_port;
    Ok(())
}

fn update_port_in_url(http_url: &mut String, port: u16) -> anyhow::Result<()> {
    let mut http_url_url = Url::parse(http_url)?;
    if let Err(()) = http_url_url.set_port(Some(port)) {
        anyhow::bail!("Wrong url, setting port is impossible");
    }
    *http_url = http_url_url.as_str().to_string();
    Ok(())
}

impl FileConfigWithDefaultName for GeneralConfig {
    const FILE_NAME: &'static str = GENERAL_FILE;
}

pub struct PortsConfig {
    pub web3_json_rpc_http_port: u16,
    pub web3_json_rpc_ws_port: u16,
    pub healthcheck_port: u16,
    pub merkle_tree_port: u16,
    pub prometheus_listener_port: u16,
}

impl PortsConfig {
    pub fn next_empty_ports_config(&self) -> PortsConfig {
        Self {
            web3_json_rpc_http_port: self.web3_json_rpc_http_port + 100,
            web3_json_rpc_ws_port: self.web3_json_rpc_ws_port + 100,
            healthcheck_port: self.healthcheck_port + 100,
            merkle_tree_port: self.merkle_tree_port + 100,
            prometheus_listener_port: self.prometheus_listener_port + 100,
        }
    }
}

impl SaveConfig for GeneralConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let bytes =
            encode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&self)?;
        Ok(shell.write_file(path, bytes)?)
    }
}

impl ReadConfig for GeneralConfig {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&path, false)
    }
}
