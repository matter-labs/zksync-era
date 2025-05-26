use std::path::{Path, PathBuf};

use serde::Serialize;
use xshell::Shell;
use zkstack_cli_common::yaml::merge_yaml;
use zksync_basic_types::pubdata_da::PubdataSendingMode;

use crate::{
    consensus::{ConsensusConfigPatch, ConsensusGenesisSpecs},
    da::AvailConfig,
    raw::{PatchedConfig, RawConfig},
    ChainConfig, ObjectStoreConfig, ObjectStoreMode,
};

pub struct RocksDbs {
    pub state_keeper: PathBuf,
    pub merkle_tree: PathBuf,
    pub protective_reads: PathBuf,
    pub basic_witness_input_producer: PathBuf,
}

pub struct FileArtifacts {
    pub prover_object_store: PathBuf,
    pub snapshot: PathBuf,
    pub core_object_store: PathBuf,
}

impl FileArtifacts {
    /// Currently all artifacts are stored in one path, but we keep an opportunity to update this paths
    pub fn new(path: PathBuf) -> Self {
        Self {
            prover_object_store: path.clone(),
            snapshot: path.clone(),
            core_object_store: path.clone(),
        }
    }
}

#[derive(Debug)]
pub struct EthSenderLimits {
    pub max_aggregated_tx_gas: u64,
    pub max_eth_tx_data_size: u64,
}

#[derive(Debug, Serialize)]
pub enum CloudConnectionMode {
    GCP,
    #[serde(rename = "LOCAL")] // match name in file-based configs
    Local,
}

#[derive(Debug)]
pub struct GeneralConfig(RawConfig);

impl GeneralConfig {
    pub async fn read(shell: &Shell, path: PathBuf) -> anyhow::Result<Self> {
        RawConfig::read(shell, path).await.map(Self)
    }

    pub fn patched(self) -> GeneralConfigPatch {
        GeneralConfigPatch(self.0.patched())
    }

    /// Obtains HTTP RPC URL for the main node based on its general config. The URL will have 127.0.0.1 host.
    pub fn l2_http_url(&self) -> anyhow::Result<String> {
        self.0.get("api.web3_json_rpc.http_url")
    }

    /// Obtains WS RPC URL for the main node based on its general config. The URL will have 127.0.0.1 host.
    pub fn l2_ws_url(&self) -> anyhow::Result<String> {
        self.0.get("api.web3_json_rpc.ws_url")
    }

    pub fn healthcheck_url(&self) -> anyhow::Result<String> {
        let port = self.0.get::<u16>("api.healthcheck.port")?;
        Ok(format!("http://127.0.0.1:{port}/health"))
    }

    pub fn contract_verifier_url(&self) -> anyhow::Result<String> {
        let port = self.0.get::<u16>("contract_verifier.port")?;
        Ok(format!("http://127.0.0.1:{port}"))
    }

    pub fn contract_verifier_prometheus_port(&self) -> anyhow::Result<u16> {
        self.0.get("contract_verifier.prometheus_port")
    }

    pub fn proof_data_handler_url(&self) -> anyhow::Result<Option<String>> {
        let port = self.0.get_opt::<u16>("data_handler.http_port")?;
        Ok(port.map(|port| format!("http://127.0.0.1:{port}")))
    }

    pub fn tee_proof_data_handler_url(&self) -> anyhow::Result<Option<String>> {
        let port = self.0.get_opt::<u16>("tee_proof_data_handler.http_port")?;
        Ok(port.map(|port| format!("http://127.0.0.1:{port}")))
    }

    pub fn prover_gateway_url(&self) -> anyhow::Result<Option<String>> {
        let port = self.0.get_opt::<u16>("prover_gateway.port")?;
        Ok(port.map(|port| format!("http://127.0.0.1:{port}")))
    }

    pub fn da_client_type(&self) -> Option<String> {
        self.0.get_opt("da_client.client").unwrap_or(None)
    }

    pub fn test_core_database_url(&self) -> anyhow::Result<String> {
        self.0.get::<String>("postgres.test.server_url")
    }

    pub fn test_prover_database_url(&self) -> anyhow::Result<String> {
        self.0.get::<String>("postgres.test.prover_url")
    }

    pub fn consensus_public_addr(&self) -> anyhow::Result<String> {
        self.0.get("consensus.public_addr")
    }

    pub fn raw_consensus_genesis_spec(&self) -> Option<&serde_yaml::Value> {
        self.0.get_raw("consensus.genesis_spec")
    }
}

#[derive(Debug)]
#[must_use = "Must be `save()`d for changes to take effect"]
pub struct GeneralConfigPatch(PatchedConfig);

impl GeneralConfigPatch {
    pub fn set_rocks_db_config(&mut self, rocks_dbs: RocksDbs) -> anyhow::Result<()> {
        self.0
            .insert_path("db.state_keeper_db_path", &rocks_dbs.state_keeper)?;
        self.0
            .insert_path("db.merkle_tree.path", &rocks_dbs.merkle_tree)?;
        self.0.insert_path(
            "protective_reads_writer.db_path",
            &rocks_dbs.protective_reads,
        )?;
        self.0.insert_path(
            "basic_witness_input_producer.db_path",
            &rocks_dbs.basic_witness_input_producer,
        )?;
        Ok(())
    }

    pub fn set_file_artifacts(&mut self, file_artifacts: FileArtifacts) -> anyhow::Result<()> {
        set_file_backed_path_if_selected(
            &mut self.0,
            "prover.prover_object_store",
            &file_artifacts.prover_object_store,
        )?;
        set_file_backed_path_if_selected(
            &mut self.0,
            "snapshot_creator.object_store",
            &file_artifacts.snapshot,
        )?;
        set_file_backed_path_if_selected(
            &mut self.0,
            "snapshot_recovery.object_store",
            &file_artifacts.snapshot,
        )?;
        set_file_backed_path_if_selected(
            &mut self.0,
            "core_object_store",
            &file_artifacts.core_object_store,
        )?;
        Ok(())
    }

    pub fn extract_consensus(
        &mut self,
        shell: &Shell,
        path: PathBuf,
    ) -> anyhow::Result<ConsensusConfigPatch> {
        let raw_consensus: serde_yaml::Mapping = self.0.base().get("consensus")?;
        self.0.remove("consensus");
        let mut new_config = PatchedConfig::empty(shell, path);
        new_config.extend(raw_consensus);
        Ok(ConsensusConfigPatch(new_config))
    }

    pub fn set_consensus_specs(&mut self, specs: ConsensusGenesisSpecs) -> anyhow::Result<()> {
        self.0
            .insert("consensus.genesis_spec.chain_id", specs.chain_id.as_u64())?;
        self.0
            .insert("consensus.genesis_spec.protocol_version", 1u64)?;
        self.0
            .insert_yaml("consensus.genesis_spec.validators", specs.validators)?;
        self.0
            .insert("consensus.genesis_spec.leader", specs.leader)?;
        Ok(())
    }

    pub fn set_prover_gateway_url(&mut self, url: String) -> anyhow::Result<()> {
        self.0.insert("prover_gateway.api_url", url)
    }

    pub fn set_tee_prover_gateway_url(&mut self, url: String) -> anyhow::Result<()> {
        self.0.insert("tee_prover_gateway.api_url", url)
    }

    pub fn set_proof_data_handler_url(&mut self, url: String) -> anyhow::Result<()> {
        self.0.insert("data_handler.gateway_api_url", url)
    }

    pub fn set_tee_proof_data_handler_url(&mut self, url: String) -> anyhow::Result<()> {
        self.0.insert("tee_proof_data_handler.gateway_api_url", url)
    }

    pub fn proof_compressor_setup_download_url(&self) -> anyhow::Result<String> {
        self.0
            .base()
            .get("proof_compressor.universal_setup_download_url")
    }

    pub fn set_proof_compressor_setup_path(&mut self, path: &Path) -> anyhow::Result<()> {
        self.0
            .insert_path("proof_compressor.universal_setup_path", path)
    }

    pub fn set_prover_setup_path(&mut self, path: &Path) -> anyhow::Result<()> {
        self.0.insert_path("prover.setup_data_path", path)
    }

    pub fn set_pubdata_sending_mode(&mut self, mode: PubdataSendingMode) -> anyhow::Result<()> {
        // `PubdataSendingMode` has differing `serde` and file-based config serializations, hence
        // we supply a raw string value.
        let raw_mode = match mode {
            PubdataSendingMode::Blobs => "BLOBS",
            PubdataSendingMode::Calldata => "CALLDATA",
            PubdataSendingMode::RelayedL2Calldata => "RELAYED_L2_CALLDATA",
            PubdataSendingMode::Custom => "CUSTOM",
        };

        self.0.insert("eth.sender.pubdata_sending_mode", raw_mode)
    }

    pub fn set_eth_sender_confirmations(&mut self, confirmations: usize) -> anyhow::Result<()> {
        self.0
            .insert("eth.sender.wait_confirmations", confirmations)
    }

    pub fn set_eth_sender_limits(&mut self, limits: EthSenderLimits) -> anyhow::Result<()> {
        self.0.insert(
            "eth.sender.max_aggregated_tx_gas",
            limits.max_aggregated_tx_gas,
        )?;
        self.0.insert(
            "eth.sender.max_eth_tx_data_size",
            limits.max_eth_tx_data_size,
        )
    }

    pub fn remove_da_client(&mut self) {
        self.0.remove("da_client");
    }

    pub fn set_avail_client(&mut self, client: &AvailConfig) -> anyhow::Result<()> {
        self.0.insert_yaml("da_client", client)?;
        self.0.insert("da_client.client", "Avail")?;
        Ok(())
    }

    pub fn set_eigendav2secure_client(&mut self) -> anyhow::Result<()> {
        self.0.insert("da_client.client", "EigenDAV2Secure")?;
        Ok(())
    }

    fn set_object_store(&mut self, prefix: &str, config: &ObjectStoreConfig) -> anyhow::Result<()> {
        self.0
            .insert(&format!("{prefix}.max_retries"), config.max_retries)?;
        match &config.mode {
            ObjectStoreMode::FileBacked {
                file_backed_base_path,
            } => {
                self.0
                    .insert_yaml(&format!("{prefix}.mode"), "FileBacked")?;
                self.0.insert_yaml(
                    &format!("{prefix}.file_backed_base_path"),
                    file_backed_base_path,
                )?;
            }
            ObjectStoreMode::GCSWithCredentialFile {
                bucket_base_url,
                gcs_credential_file_path,
            } => {
                self.0
                    .insert(&format!("{prefix}.mode"), "GCSWithCredentialFile")?;
                self.0.insert(
                    &format!("{prefix}.bucket_base_url"),
                    bucket_base_url.clone(),
                )?;
                self.0.insert(
                    &format!("{prefix}.gcs_credential_file_path"),
                    gcs_credential_file_path.clone(),
                )?;
            }
        }
        Ok(())
    }

    pub fn set_prover_object_store(&mut self, config: &ObjectStoreConfig) -> anyhow::Result<()> {
        self.set_object_store("prover.prover_object_store", config)
    }

    pub async fn save(self) -> anyhow::Result<()> {
        self.0.save().await
    }
}

fn set_file_backed_path_if_selected(
    config: &mut PatchedConfig,
    prefix: &str,
    path: &Path,
) -> anyhow::Result<()> {
    let mode_path = format!("{prefix}.mode");
    let mode = config.base().get_opt::<String>(&mode_path)?;
    let is_file_backed = mode.as_ref().is_none_or(|mode| mode == "FileBacked");

    if is_file_backed {
        config.insert(&mode_path, "FileBacked")?;
        config.insert_path(&format!("{prefix}.file_backed_base_path"), path)?;
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
