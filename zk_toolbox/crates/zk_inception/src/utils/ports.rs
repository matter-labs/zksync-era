use std::{
    collections::{HashMap, HashSet},
    fmt,
    ops::Range,
    path::Path,
};

use anyhow::{bail, Context, Result};
use config::{
    explorer_compose::ExplorerBackendPorts, update_port_in_host, update_port_in_url,
    EcosystemConfig, GeneralConfig, DEFAULT_CONSENSUS_PORT, DEFAULT_CONTRACT_VERIFIER_PORT,
    DEFAULT_EXPLORER_API_PORT, DEFAULT_EXPLORER_DATA_FETCHER_PORT, DEFAULT_EXPLORER_WORKER_PORT,
    DEFAULT_HEALTHCHECK_PORT, DEFAULT_MERKLE_TREE_PORT, DEFAULT_PROMETHEUS_PORT,
    DEFAULT_WEB3_JSON_RPC_PORT, DEFAULT_WEB3_WS_RPC_PORT,
};
use serde_yaml::Value;
use xshell::Shell;

use crate::defaults::{DEFAULT_OBSERVABILITY_PORT, PORT_RANGE_END, PORT_RANGE_START};

pub struct EcosystemPorts {
    pub ports: HashMap<u16, Vec<String>>,
}

impl EcosystemPorts {
    pub fn get_assigned_ports(&self) -> HashSet<u16> {
        self.ports.keys().cloned().collect()
    }

    pub fn is_port_assigned(&self, port: u16) -> bool {
        self.ports.contains_key(&port)
    }

    pub fn add_port_info(&mut self, port: u16, info: String) {
        self.ports.entry(port).or_default().push(info);
    }

    pub fn allocate_port(&mut self, range: Range<u16>, info: String) -> anyhow::Result<u16> {
        for port in range {
            if !self.is_port_assigned(port) {
                self.add_port_info(port, info.to_string());
                return Ok(port);
            }
        }
        anyhow::bail!(format!(
            "No available ports in the given range. Failed to allocate port for: {}",
            info
        ));
    }

    pub fn allocate_ports_with_offset_from_defaults<T: ConfigWithChainPorts>(
        &mut self,
        config: &mut T,
        chain_number: u32,
    ) -> Result<()> {
        let offset = ((chain_number - 1) as u16) * 100;
        let port_range = (PORT_RANGE_START + offset)..PORT_RANGE_END;

        let mut new_ports = HashMap::new();
        for (desc, port) in T::get_default_ports() {
            let mut new_port = port + offset;
            if self.is_port_assigned(new_port) {
                new_port = self.allocate_port(port_range.clone(), desc.clone())?;
            } else {
                self.add_port_info(new_port, desc.to_string());
            }
            new_ports.insert(desc, new_port);
        }
        config.set_ports(new_ports)?;
        Ok(())
    }
}

impl fmt::Display for EcosystemPorts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut port_vec: Vec<_> = self.ports.iter().collect();
        port_vec.sort_by_key(|&(key, _)| key);
        for (port, port_infos) in port_vec {
            for port_info in port_infos {
                writeln!(f, "{} > {}", port_info, port)?;
            }
        }
        Ok(())
    }
}

impl Default for EcosystemPorts {
    fn default() -> Self {
        let mut ports = HashMap::new();
        ports.insert(
            DEFAULT_OBSERVABILITY_PORT,
            vec!["Observability".to_string()],
        );
        Self {
            ports: HashMap::new(),
        }
    }
}

pub struct EcosystemPortsScanner {}

impl EcosystemPortsScanner {
    /// Scans the ecosystem directory for YAML files and extracts port information.
    /// Specifically, it looks for keys ending with "port" and collects their values.
    /// Note: Port information from Docker Compose files will not be picked up by this method.
    pub fn scan(shell: &Shell) -> Result<EcosystemPorts> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;

        // Create a list of directories to scan:
        // - Ecosystem configs directory
        // - Chain configs directories
        // - ZKsync repo directory (docker-compose files)
        let mut dirs = vec![ecosystem_config.config.clone()];
        for chain in ecosystem_config.list_of_chains() {
            if let Some(chain_config) = ecosystem_config.load_chain(Some(chain)) {
                dirs.push(chain_config.configs.clone())
            }
        }
        dirs.push(ecosystem_config.link_to_code);

        let mut ecosystem_ports = EcosystemPorts::default();
        for dir in dirs {
            if dir.is_dir() {
                Self::scan_yaml_files(shell, &dir, &mut ecosystem_ports)
                    .context(format!("Failed to scan directory {:?}", dir))?;
            }
        }

        Ok(ecosystem_ports)
    }

    /// Scans the given directory for YAML files in the immediate directory only (non-recursive).
    /// Processes each YAML file found and updates the EcosystemPorts accordingly.
    fn scan_yaml_files(
        shell: &Shell,
        dir: &Path,
        ecosystem_ports: &mut EcosystemPorts,
    ) -> Result<()> {
        for path in shell.read_dir(dir)? {
            if !path.is_file() {
                continue;
            }
            if let Some(extension) = path.extension() {
                if extension == "yaml" || extension == "yml" {
                    Self::process_yaml_file(shell, &path, ecosystem_ports)
                        .context(format!("Error processing YAML file {:?}", path))?;
                }
            }
        }
        Ok(())
    }

    fn process_yaml_file(
        shell: &Shell,
        file_path: &Path,
        ecosystem_ports: &mut EcosystemPorts,
    ) -> Result<()> {
        let contents = shell.read_file(file_path)?;
        let value: Value = serde_yaml::from_str(&contents)?;
        Self::traverse_yaml(&value, "", file_path, ecosystem_ports);
        Ok(())
    }

    fn traverse_yaml(
        value: &Value,
        path: &str,
        file_path: &Path,
        ecosystem_ports: &mut EcosystemPorts,
    ) {
        match value {
            Value::Mapping(map) => {
                for (key, val) in map {
                    let new_path = if path.is_empty() {
                        key.as_str().unwrap_or_default().to_string()
                    } else {
                        format!("{}:{}", path, key.as_str().unwrap_or_default())
                    };

                    if key.as_str() == Some("ports") {
                        Self::process_docker_compose_ports(
                            val,
                            &new_path,
                            file_path,
                            ecosystem_ports,
                        );
                    } else if key.as_str().map(|s| s.ends_with("port")).unwrap_or(false) {
                        Self::process_general_config_port(
                            val,
                            &new_path,
                            file_path,
                            ecosystem_ports,
                        );
                    }

                    Self::traverse_yaml(val, &new_path, file_path, ecosystem_ports);
                }
            }
            Value::Sequence(seq) => {
                for (index, val) in seq.iter().enumerate() {
                    let new_path = format!("{}:{}", path, index);
                    Self::traverse_yaml(val, &new_path, file_path, ecosystem_ports);
                }
            }
            _ => {}
        }
    }

    fn process_general_config_port(
        value: &Value,
        path: &str,
        file_path: &Path,
        ecosystem_ports: &mut EcosystemPorts,
    ) {
        if let Some(port) = value.as_u64().and_then(|p| u16::try_from(p).ok()) {
            let description = format!("[{}] {}", file_path.display(), path);
            ecosystem_ports.add_port_info(port, description);
        }
    }

    fn process_docker_compose_ports(
        value: &Value,
        path: &str,
        file_path: &Path,
        ecosystem_ports: &mut EcosystemPorts,
    ) {
        if let Value::Sequence(ports) = value {
            for port_entry in ports {
                if let Some(port_str) = port_entry.as_str() {
                    if let Some(port) = Self::parse_host_port(port_str) {
                        Self::add_parsed_port(port, path, file_path, ecosystem_ports);
                    }
                }
            }
        }
    }

    fn parse_host_port(port_str: &str) -> Option<u16> {
        let parts: Vec<&str> = port_str.split(':').collect();
        if parts.len() > 1 {
            if let Some(host_port_str) = parts.get(parts.len() - 2) {
                if let Ok(port) = host_port_str.parse::<u16>() {
                    return Some(port);
                }
            }
        }
        None
    }

    fn add_parsed_port(
        port: u16,
        path: &str,
        file_path: &Path,
        ecosystem_ports: &mut EcosystemPorts,
    ) {
        let description = format!("[{}] {}", file_path.display(), path);
        ecosystem_ports.add_port_info(port, description);
    }
}

pub trait ConfigWithChainPorts {
    fn get_default_ports() -> HashMap<String, u16>;
    fn set_ports(&mut self, ports: HashMap<String, u16>) -> Result<()>;
}

impl ConfigWithChainPorts for ExplorerBackendPorts {
    fn get_default_ports() -> HashMap<String, u16> {
        HashMap::from([
            ("api_http_port".to_string(), DEFAULT_EXPLORER_API_PORT),
            (
                "data_fetcher_http_port".to_string(),
                DEFAULT_EXPLORER_DATA_FETCHER_PORT,
            ),
            ("worker_http_port".to_string(), DEFAULT_EXPLORER_WORKER_PORT),
        ])
    }

    fn set_ports(&mut self, ports: HashMap<String, u16>) -> anyhow::Result<()> {
        if ports.len() != Self::get_default_ports().len() {
            bail!("Incorrect number of ports provided");
        }
        for (desc, port) in ports {
            match desc.as_str() {
                "api_http_port" => self.api_http_port = port,
                "data_fetcher_http_port" => self.data_fetcher_http_port = port,
                "worker_http_port" => self.worker_http_port = port,
                _ => bail!("Unknown port descriptor: {}", desc),
            }
        }
        Ok(())
    }
}

impl ConfigWithChainPorts for GeneralConfig {
    fn get_default_ports() -> HashMap<String, u16> {
        HashMap::from([
            (
                "web3_json_rpc_http_port".to_string(),
                DEFAULT_WEB3_JSON_RPC_PORT,
            ),
            (
                "web3_json_rpc_ws_port".to_string(),
                DEFAULT_WEB3_WS_RPC_PORT,
            ),
            ("healthcheck_port".to_string(), DEFAULT_HEALTHCHECK_PORT),
            ("merkle_tree_port".to_string(), DEFAULT_MERKLE_TREE_PORT),
            (
                "prometheus_listener_port".to_string(),
                DEFAULT_PROMETHEUS_PORT,
            ),
            (
                "contract_verifier_port".to_string(),
                DEFAULT_CONTRACT_VERIFIER_PORT,
            ),
            ("consensus_port".to_string(), DEFAULT_CONSENSUS_PORT),
        ])
    }

    fn set_ports(&mut self, ports: HashMap<String, u16>) -> anyhow::Result<()> {
        if ports.len() != Self::get_default_ports().len() {
            bail!("Incorrect number of ports provided");
        }

        let api = self
            .api_config
            .as_mut()
            .context("Api config is not presented")?;
        let contract_verifier = self
            .contract_verifier
            .as_mut()
            .context("Contract Verifier config is not presented")?;
        let prometheus = self
            .prometheus_config
            .as_mut()
            .context("Prometheus config is not presented")?;

        for (desc, port) in ports {
            match desc.as_str() {
                "web3_json_rpc_http_port" => {
                    api.web3_json_rpc.http_port = port;
                    update_port_in_url(&mut api.web3_json_rpc.http_url, port)?;
                }
                "web3_json_rpc_ws_port" => {
                    api.web3_json_rpc.ws_port = port;
                    update_port_in_url(&mut api.web3_json_rpc.ws_url, port)?;
                }
                "healthcheck_port" => api.healthcheck.port = port,
                "merkle_tree_port" => api.merkle_tree.port = port,
                "prometheus_listener_port" => prometheus.listener_port = port,
                "contract_verifier_port" => {
                    contract_verifier.port = port;
                    update_port_in_url(&mut contract_verifier.url, port)?;
                }
                "consensus_port" => {
                    if let Some(consensus) = self.consensus_config.as_mut() {
                        consensus.server_addr.set_port(port);
                        update_port_in_host(&mut consensus.public_addr, port)?;
                    }
                }
                _ => bail!("Unknown port descriptor: {}", desc),
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::utils::ports::{EcosystemPorts, EcosystemPortsScanner};

    #[test]
    fn test_traverse_yaml() {
        let yaml_content = r#"
            api:
                web3_json_rpc:
                    http_port: 3050
                    ws_port: 3051
                    api_namespaces:
                    - eth
                    gas_price_scale_factor: 1.5
                prometheus:
                    listener_port: 3412
                    push_interval_ms: 100
            contract_verifier:
                port: 3070
            prometheus:
                listener_port: 3412
            reth:
                image: "ghcr.io/paradigmxyz/reth:v1.0.6"
                ports:
                    - 127.0.0.1:8546:8545
            postgres:
                image: "postgres:14"
                ports:
                    - "5433:5432"
            
        "#;

        let value = serde_yaml::from_str(yaml_content).unwrap();
        let mut ecosystem_ports = EcosystemPorts::default();
        let file_path = PathBuf::from("test_config.yaml");

        EcosystemPortsScanner::traverse_yaml(&value, "", &file_path, &mut ecosystem_ports);

        // Assigned ports:
        assert!(ecosystem_ports.is_port_assigned(3050));
        assert!(ecosystem_ports.is_port_assigned(3051));
        assert!(ecosystem_ports.is_port_assigned(3070));
        assert!(ecosystem_ports.is_port_assigned(3412));
        assert!(ecosystem_ports.is_port_assigned(8546));
        assert!(ecosystem_ports.is_port_assigned(5433));

        // Free ports:
        assert!(!ecosystem_ports.is_port_assigned(3150));
        assert!(!ecosystem_ports.is_port_assigned(3151));
        assert!(!ecosystem_ports.is_port_assigned(8545));
        assert!(!ecosystem_ports.is_port_assigned(5432));

        // Check description:
        let port_3050_info = ecosystem_ports.ports.get(&3050).unwrap();
        assert_eq!(port_3050_info.len(), 1);
        assert_eq!(
            port_3050_info[0],
            "[test_config.yaml] api:web3_json_rpc:http_port"
        );

        let port_3412_info = ecosystem_ports.ports.get(&3412).unwrap();
        assert_eq!(port_3412_info.len(), 2);
        assert_eq!(
            port_3412_info[0],
            "[test_config.yaml] api:prometheus:listener_port"
        );
        assert_eq!(
            port_3412_info[1],
            "[test_config.yaml] prometheus:listener_port"
        );
    }

    #[test]
    fn test_process_port_value() {
        let yaml_content = r#"
        web3_json_rpc:
            http_port: 3050
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_content).unwrap();
        let mut ecosystem_ports = EcosystemPorts::default();
        let file_path = PathBuf::from("test_config.yaml");

        EcosystemPortsScanner::process_general_config_port(
            &value["web3_json_rpc"]["http_port"],
            "web3_json_rpc:http_port",
            &file_path,
            &mut ecosystem_ports,
        );

        assert!(ecosystem_ports.is_port_assigned(3050));
        let port_info = ecosystem_ports.ports.get(&3050).unwrap();
        assert_eq!(port_info[0], "[test_config.yaml] web3_json_rpc:http_port");
    }

    #[test]
    fn test_parse_process_docker_compose_ports() {
        assert_eq!(
            EcosystemPortsScanner::parse_host_port("127.0.0.1:8546:8545"),
            Some(8546)
        );
        assert_eq!(
            EcosystemPortsScanner::parse_host_port("5433:5432"),
            Some(5433)
        );
        assert_eq!(EcosystemPortsScanner::parse_host_port("localhost:80"), None);
        assert_eq!(EcosystemPortsScanner::parse_host_port("8080"), None);
    }

    #[test]
    fn test_add_parsed_port() {
        let mut ecosystem_ports = EcosystemPorts::default();
        let file_path = PathBuf::from("test_config.yaml");

        EcosystemPortsScanner::add_parsed_port(
            8546,
            "reth:ports",
            &file_path,
            &mut ecosystem_ports,
        );

        assert!(ecosystem_ports.is_port_assigned(8546));
        let port_info = ecosystem_ports.ports.get(&8546).unwrap();
        assert_eq!(port_info[0], "[test_config.yaml] reth:ports");
    }
}
