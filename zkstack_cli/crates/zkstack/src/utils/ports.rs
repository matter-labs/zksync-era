use std::{collections::HashMap, fmt, net::SocketAddr, ops::Range, path::Path};

use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use url::Url;
use xshell::Shell;
use zkstack_cli_config::{
    explorer_compose::ExplorerBackendPorts, EcosystemConfig, DEFAULT_EXPLORER_API_PORT,
    DEFAULT_EXPLORER_DATA_FETCHER_PORT, DEFAULT_EXPLORER_WORKER_PORT,
};

use crate::defaults::{DEFAULT_OBSERVABILITY_PORT, PORT_RANGE_END, PORT_RANGE_START};

pub struct EcosystemPorts {
    pub ports: HashMap<u16, Vec<PortInfo>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortInfo {
    pub port: u16,
    pub file_path: String,
    pub description: String,
}

impl fmt::Display for PortInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} >{}",
            self.file_path, self.description, self.port
        )
    }
}

impl EcosystemPorts {
    pub fn is_port_assigned(&self, port: u16) -> bool {
        self.ports.contains_key(&port)
    }

    pub fn add_port_info(&mut self, port: u16, info: PortInfo) {
        let info = PortInfo {
            port,
            file_path: info.file_path,
            description: info.description,
        };
        self.ports.entry(port).or_default().push(info);
    }

    pub fn allocate_port(&mut self, range: Range<u16>, info: PortInfo) -> anyhow::Result<u16> {
        for port in range {
            if !self.is_port_assigned(port) {
                self.add_port_info(port, info);
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
        for (desc, port) in config.get_default_ports()? {
            let mut new_port = port + offset;
            let port_info = PortInfo {
                port: new_port,
                description: desc.clone(),
                ..Default::default()
            };
            if self.is_port_assigned(new_port) {
                new_port = self.allocate_port(port_range.clone(), port_info)?;
            } else {
                self.add_port_info(new_port, port_info);
            }
            new_ports.insert(desc, new_port);
        }
        config.set_ports(new_ports)?;
        Ok(())
    }

    pub fn allocate_ports_in_yaml(
        &mut self,
        shell: &Shell,
        file_path: &Path,
        chain_number: u32,
        tight_ports: bool,
    ) -> Result<()> {
        let file_contents = shell.read_file(file_path)?;
        let mut value: Value = serde_yaml::from_str(&file_contents)?;
        let offset = if tight_ports {
            0
        } else {
            (chain_number - 1) * 100
        };
        self.traverse_allocate_ports_in_yaml(&mut value, offset, file_path)?;
        let new_contents = serde_yaml::to_string(&value)?;
        if new_contents != file_contents {
            shell.write_file(file_path, new_contents)?;
        }
        Ok(())
    }

    fn traverse_allocate_ports_in_yaml(
        &mut self,
        value: &mut Value,
        offset: u32,
        file_path: &Path,
    ) -> anyhow::Result<()> {
        match value {
            Value::Mapping(map) => {
                let mut updated_ports = HashMap::new();
                for (key, val) in &mut *map {
                    if key.as_str().map(|s| s.ends_with("port")).unwrap_or(false) {
                        if let Some(port) = val.as_u64().and_then(|p| u16::try_from(p).ok()) {
                            let new_port = self.allocate_port(
                                (port + offset as u16)..PORT_RANGE_END,
                                PortInfo {
                                    port,
                                    file_path: file_path.display().to_string(),
                                    description: key.as_str().unwrap_or_default().to_string(),
                                },
                            )?;
                            *val = Value::Number(serde_yaml::Number::from(new_port));
                            updated_ports.insert(port, new_port);
                        }
                    }
                }
                // Update ports in URLs
                for (key, val) in &mut *map {
                    if key.as_str().map(|s| s.ends_with("url")).unwrap_or(false) {
                        let mut url = Url::parse(val.as_str().unwrap())?;
                        if let Some(port) = url.port() {
                            if let Some(new_port) = updated_ports.get(&port) {
                                if let Err(()) = url.set_port(Some(*new_port)) {
                                    bail!("Failed to update port in URL {}", url);
                                } else {
                                    *val = Value::String(url.to_string());
                                }
                            }
                        }
                    } else if key.as_str().map(|s| s.ends_with("addr")).unwrap_or(false) {
                        let socket_addr = val.as_str().unwrap().parse::<SocketAddr>()?;
                        if let Some(new_port) = updated_ports.get(&socket_addr.port()) {
                            let new_socket_addr = SocketAddr::new(socket_addr.ip(), *new_port);
                            *val = Value::String(new_socket_addr.to_string());
                        }
                    }
                }
                // Continue traversing
                for (_, val) in map {
                    self.traverse_allocate_ports_in_yaml(val, offset, file_path)?;
                }
            }
            Value::Sequence(seq) => {
                for val in seq {
                    self.traverse_allocate_ports_in_yaml(val, offset, file_path)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn group_by_file_path(&self) -> HashMap<String, Vec<PortInfo>> {
        let mut grouped_ports: HashMap<String, Vec<PortInfo>> = HashMap::new();
        for port_infos in self.ports.values() {
            for port_info in port_infos {
                grouped_ports
                    .entry(port_info.file_path.clone())
                    .or_default()
                    .push(port_info.clone());
            }
        }
        grouped_ports
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

pub struct EcosystemPortsScanner;

impl EcosystemPortsScanner {
    /// Scans the ecosystem directory for YAML files and extracts port information.
    /// Specifically, it looks for keys ending with "port" or "ports" and collects their values.
    /// We could skip searching ports in the current chain if we initialize this chain.
    /// It allows to reuse default ports even if the general file exist with previously allocated ports.
    /// It makes allocation more predictable
    pub fn scan(shell: &Shell, current_chain: Option<&str>) -> Result<EcosystemPorts> {
        let ecosystem_config = EcosystemConfig::from_file(shell)?;

        // Create a list of directories to scan:
        // - Ecosystem configs directory
        // - Chain configs directories
        // - External node directories
        // - Ecosystem directory (docker-compose files)
        let mut dirs = vec![ecosystem_config.config.clone()];
        for chain in ecosystem_config.list_of_chains() {
            // skip current chain for avoiding wrong reallocation
            if let Some(name) = current_chain {
                if name.eq(&chain) {
                    continue;
                }
            }
            if let Ok(chain_config) = ecosystem_config.load_chain(Some(chain)) {
                dirs.push(chain_config.configs.clone());
                if let Some(external_node_config_path) = &chain_config.external_node_config_path {
                    dirs.push(external_node_config_path.clone());
                }
            }
        }
        dirs.push(shell.current_dir()); // Ecosystem directory

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
            let info = PortInfo {
                port,
                file_path: file_path.display().to_string(),
                description: path.to_string(),
            };
            ecosystem_ports.add_port_info(port, info);
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
        let info = PortInfo {
            port,
            file_path: file_path.display().to_string(),
            description: path.to_string(),
        };
        ecosystem_ports.add_port_info(port, info);
    }
}

pub trait ConfigWithChainPorts {
    fn get_default_ports(&self) -> anyhow::Result<HashMap<String, u16>>;
    fn set_ports(&mut self, ports: HashMap<String, u16>) -> Result<()>;
}

impl ConfigWithChainPorts for ExplorerBackendPorts {
    fn get_default_ports(&self) -> anyhow::Result<HashMap<String, u16>> {
        Ok(HashMap::from([
            ("api_http_port".to_string(), DEFAULT_EXPLORER_API_PORT),
            (
                "data_fetcher_http_port".to_string(),
                DEFAULT_EXPLORER_DATA_FETCHER_PORT,
            ),
            ("worker_http_port".to_string(), DEFAULT_EXPLORER_WORKER_PORT),
        ]))
    }

    fn set_ports(&mut self, ports: HashMap<String, u16>) -> anyhow::Result<()> {
        if ports.len() != self.get_default_ports()?.len() {
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::utils::ports::{EcosystemPorts, EcosystemPortsScanner, PortInfo};

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
                image: "ghcr.io/paradigmxyz/reth:v1.3.7"
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
        assert!(!ecosystem_ports.is_port_assigned(100));

        // Check description:
        let port_3050_info = ecosystem_ports.ports.get(&3050).unwrap();
        assert_eq!(port_3050_info.len(), 1);
        let expected_port_3050_info = PortInfo {
            port: 3050,
            file_path: "test_config.yaml".to_string(),
            description: "api:web3_json_rpc:http_port".to_string(),
        };
        assert_eq!(port_3050_info[0], expected_port_3050_info);

        let port_3412_info = ecosystem_ports.ports.get(&3412).unwrap();
        assert_eq!(port_3412_info.len(), 2);
        let expected_port_3412_info_0 = PortInfo {
            port: 3412,
            file_path: "test_config.yaml".to_string(),
            description: "api:prometheus:listener_port".to_string(),
        };
        let expected_port_3412_info_1 = PortInfo {
            port: 3412,
            file_path: "test_config.yaml".to_string(),
            description: "prometheus:listener_port".to_string(),
        };

        assert_eq!(port_3412_info[0], expected_port_3412_info_0);
        assert_eq!(port_3412_info[1], expected_port_3412_info_1);
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
        let expected_port_info = PortInfo {
            port: 3050,
            file_path: "test_config.yaml".to_string(),
            description: "web3_json_rpc:http_port".to_string(),
        };
        assert_eq!(port_info[0], expected_port_info);
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
        let expected_port_info = PortInfo {
            port: 8546,
            file_path: "test_config.yaml".to_string(),
            description: "reth:ports".to_string(),
        };
        assert_eq!(port_info[0], expected_port_info);
    }

    #[test]
    fn test_traverse_allocate_ports_in_yaml_with_chain_number_zero() {
        let yaml_content = r#"
            api:
                web3_json_rpc:
                    http_port: 3050
                    http_url: http://127.0.0.1:3050
                    ws_port: 3051
                    ws_url: ws://127.0.0.1:3051
                    gas_price_scale_factor: 1.5
                prometheus:
                    listener_port: 3412
                    pushgateway_url: http://127.0.0.1:9091
                    push_interval_ms: 100
        "#;

        let mut value = serde_yaml::from_str(yaml_content).unwrap();
        let mut ecosystem_ports = EcosystemPorts::default();
        let chain_number = 0;
        let offset = chain_number * 100;
        ecosystem_ports
            .traverse_allocate_ports_in_yaml(&mut value, offset, Path::new("."))
            .unwrap();

        let api = value["api"].as_mapping().unwrap();
        let web3_json_rpc = api["web3_json_rpc"].as_mapping().unwrap();
        let prometheus = api["prometheus"].as_mapping().unwrap();

        assert_eq!(web3_json_rpc["http_port"].as_u64().unwrap(), 3050);
        assert_eq!(web3_json_rpc["ws_port"].as_u64().unwrap(), 3051);
        assert_eq!(prometheus["listener_port"].as_u64().unwrap(), 3412);
        assert_eq!(
            web3_json_rpc["http_url"].as_str().unwrap(),
            "http://127.0.0.1:3050/"
        );
        assert_eq!(
            web3_json_rpc["ws_url"].as_str().unwrap(),
            "ws://127.0.0.1:3051/"
        );
        assert_eq!(
            prometheus["pushgateway_url"].as_str().unwrap(),
            "http://127.0.0.1:9091"
        );
    }

    #[test]
    fn test_traverse_allocate_ports_in_yaml_with_chain_number_one() {
        let yaml_content = r#"
            api:
                web3_json_rpc:
                    http_port: 3050
                    http_url: http://127.0.0.1:3050
                    ws_port: 3051
                    ws_url: ws://127.0.0.1:3051
                    gas_price_scale_factor: 1.5
                prometheus:
                    listener_port: 3412
                    pushgateway_url: http://127.0.0.1:9091
                    push_interval_ms: 100
        "#;

        let mut value = serde_yaml::from_str(yaml_content).unwrap();
        let mut ecosystem_ports = EcosystemPorts::default();
        let chain_number = 1;
        let offset = chain_number * 100;
        ecosystem_ports
            .traverse_allocate_ports_in_yaml(&mut value, offset, Path::new("."))
            .unwrap();

        let api = value["api"].as_mapping().unwrap();
        let web3_json_rpc = api["web3_json_rpc"].as_mapping().unwrap();
        let prometheus = api["prometheus"].as_mapping().unwrap();

        assert_eq!(web3_json_rpc["http_port"].as_u64().unwrap(), 3150);
        assert_eq!(web3_json_rpc["ws_port"].as_u64().unwrap(), 3151);
        assert_eq!(prometheus["listener_port"].as_u64().unwrap(), 3512);
        assert_eq!(
            web3_json_rpc["http_url"].as_str().unwrap(),
            "http://127.0.0.1:3150/"
        );
        assert_eq!(
            web3_json_rpc["ws_url"].as_str().unwrap(),
            "ws://127.0.0.1:3151/"
        );
        assert_eq!(
            prometheus["pushgateway_url"].as_str().unwrap(),
            "http://127.0.0.1:9091"
        );
    }
}
