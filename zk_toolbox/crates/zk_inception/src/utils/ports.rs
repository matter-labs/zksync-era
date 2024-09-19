use std::{
    collections::{HashMap, HashSet},
    fmt,
    ops::Range,
    path::Path,
};

use anyhow::{Context, Result};
use config::{EcosystemConfig, PortsConfig};
use serde_yaml::Value;
use xshell::Shell;

use crate::defaults::PORT_RANGE;

#[derive(Default)]
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

    pub fn allocate_ports(
        &mut self,
        general_config: &mut config::GeneralConfig,
    ) -> anyhow::Result<()> {
        let ports = PortsConfig {
            web3_json_rpc_http_port: self
                .allocate_port(PORT_RANGE, "Web3 JSON RPC HTTP".to_string())?,
            web3_json_rpc_ws_port: self
                .allocate_port(PORT_RANGE, "Web3 JSON RPC WS".to_string())?,
            healthcheck_port: self.allocate_port(PORT_RANGE, "Healthcheck".to_string())?,
            merkle_tree_port: self.allocate_port(PORT_RANGE, "Merkle Tree".to_string())?,
            prometheus_listener_port: self.allocate_port(PORT_RANGE, "Prometheus".to_string())?,
            contract_verifier_port: self
                .allocate_port(PORT_RANGE, "Contract Verifier".to_string())?,
            consensus_port: self.allocate_port(PORT_RANGE, "Consensus".to_string())?,
        };

        config::update_ports(general_config, &ports)?;

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
                        if let Value::Sequence(ports) = val {
                            for port_entry in ports {
                                if let Some(port_str) = port_entry.as_str() {
                                    let parts: Vec<&str> = port_str.split(':').collect();

                                    // Handle 3-part (IP:host:container) or 2-part (host:container) port mappings
                                    let host_port_str = match parts.len() {
                                        3 => parts.get(1),
                                        2 => parts.first(),
                                        _ => None,
                                    };

                                    if let Some(host_port) = host_port_str {
                                        if let Ok(port) = host_port.parse::<u16>() {
                                            let description =
                                                format!("[{}] {}", file_path.display(), new_path);
                                            ecosystem_ports.add_port_info(port, description);
                                        }
                                    }
                                }
                            }
                        }
                    } else if key.as_str().map(|s| s.ends_with("port")).unwrap_or(false) {
                        if let Some(port) = val.as_u64().and_then(|p| u16::try_from(p).ok()) {
                            let description = format!("[{}] {}", file_path.display(), new_path);
                            ecosystem_ports.add_port_info(port, description);
                        }
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
}
