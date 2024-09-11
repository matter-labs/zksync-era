use std::path::{Path, PathBuf};

use anyhow::Context;
use url::Url;
use xshell::Shell;
pub use zksync_config::configs::GeneralConfig;
use zksync_config::configs::{consensus::Host, object_store::ObjectStoreMode};
use zksync_protobuf_config::{decode_yaml_repr, encode_yaml_repr};

use crate::{
    consts::GENERAL_FILE,
    traits::{ConfigWithL2RpcUrl, FileConfigWithDefaultName, ReadConfig, SaveConfig},
    DEFAULT_CONSENSUS_PORT,
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

pub fn ports_config(config: &GeneralConfig) -> Option<PortsConfig> {
    let api = config.api_config.as_ref()?;
    let contract_verifier = config.contract_verifier.as_ref()?;
    let consensus_port = if let Some(consensus_config) = config.clone().consensus_config {
        consensus_config.server_addr.port()
    } else {
        DEFAULT_CONSENSUS_PORT
    };

    Some(PortsConfig {
        web3_json_rpc_http_port: api.web3_json_rpc.http_port,
        web3_json_rpc_ws_port: api.web3_json_rpc.ws_port,
        healthcheck_port: api.healthcheck.port,
        merkle_tree_port: api.merkle_tree.port,
        prometheus_listener_port: api.prometheus.listener_port,
        contract_verifier_port: contract_verifier.port,
        consensus_port,
    })
}

pub fn update_ports(config: &mut GeneralConfig, ports_config: &PortsConfig) -> anyhow::Result<()> {
    let api = config
        .api_config
        .as_mut()
        .context("Api config is not presented")?;
    let contract_verifier = config
        .contract_verifier
        .as_mut()
        .context("Contract Verifier config is not presented")?;
    let prometheus = config
        .prometheus_config
        .as_mut()
        .context("Prometheus config is not presented")?;
    if let Some(consensus) = config.consensus_config.as_mut() {
        consensus.server_addr.set_port(ports_config.consensus_port);
        update_port_in_host(&mut consensus.public_addr, ports_config.consensus_port)?;
    }

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
    contract_verifier.port = ports_config.contract_verifier_port;
    update_port_in_url(
        &mut contract_verifier.url,
        ports_config.contract_verifier_port,
    )?;
    api.healthcheck.port = ports_config.healthcheck_port;
    api.merkle_tree.port = ports_config.merkle_tree_port;
    api.prometheus.listener_port = ports_config.prometheus_listener_port;

    prometheus.listener_port = ports_config.prometheus_listener_port;

    Ok(())
}

fn update_port_in_url(http_url: &mut String, port: u16) -> anyhow::Result<()> {
    let mut http_url_url = Url::parse(http_url)?;
    if let Err(()) = http_url_url.set_port(Some(port)) {
        anyhow::bail!("Wrong url, setting port is impossible");
    }
    *http_url = http_url_url.to_string();
    Ok(())
}

fn update_port_in_host(host: &mut Host, port: u16) -> anyhow::Result<()> {
    let url = Url::parse(&format!("http://{}", host.0))?;
    let host_str = url.host_str().context("Failed to get host")?;
    host.0 = format!("{host_str}:{port}");
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
    pub contract_verifier_port: u16,
    pub consensus_port: u16,
}

impl PortsConfig {
    pub fn apply_offset(&mut self, offset: u16) {
        self.web3_json_rpc_http_port += offset;
        self.web3_json_rpc_ws_port += offset;
        self.healthcheck_port += offset;
        self.merkle_tree_port += offset;
        self.prometheus_listener_port += offset;
        self.contract_verifier_port += offset;
        self.consensus_port += offset;
    }

    pub fn next_empty_ports_config(&self) -> PortsConfig {
        Self {
            web3_json_rpc_http_port: self.web3_json_rpc_http_port + 100,
            web3_json_rpc_ws_port: self.web3_json_rpc_ws_port + 100,
            healthcheck_port: self.healthcheck_port + 100,
            merkle_tree_port: self.merkle_tree_port + 100,
            prometheus_listener_port: self.prometheus_listener_port + 100,
            contract_verifier_port: self.contract_verifier_port + 100,
            consensus_port: self.consensus_port + 100,
        }
    }
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
        decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&path, false)
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
