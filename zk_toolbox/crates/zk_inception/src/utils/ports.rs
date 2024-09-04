use std::{
    collections::{HashMap, HashSet},
    fmt,
    ops::Range,
    path::Path,
};

use anyhow::{Context, Result};
use config::EcosystemConfig;
use serde_yaml::Value;
use xshell::Shell;

use crate::defaults::{
    LOCAL_HTTP_RPC_PORT, LOCAL_WS_RPC_PORT, OBSERVABILITY_PORT, POSTGRES_DB_PORT,
};

#[derive(Default)]
pub struct EcosystemPorts {
    pub ports: HashMap<u16, Vec<String>>,
}

impl EcosystemPorts {
    /// Creates a new EcosystemPorts instance with default port configurations.
    /// These ports are not configured in ecosystem yaml file configs, but are
    /// commonly used or pre-assigned within the ecosystem.
    pub fn with_default_ports() -> Self {
        let mut ports = HashMap::new();
        ports.insert(OBSERVABILITY_PORT, vec!["Observability".to_string()]);
        ports.insert(POSTGRES_DB_PORT, vec!["PostgreSQL".to_string()]);
        ports.insert(LOCAL_HTTP_RPC_PORT, vec!["Local HTTP RPC".to_string()]);
        ports.insert(LOCAL_WS_RPC_PORT, vec!["Local WebSockets RPC".to_string()]);
        Self { ports }
    }

    pub fn get_assigned_ports(&self) -> HashSet<u16> {
        self.ports.keys().cloned().collect()
    }

    pub fn is_port_assigned(&self, port: u16) -> bool {
        self.ports.contains_key(&port)
    }

    pub fn add_port_info(&mut self, port: u16, info: String) {
        self.ports.entry(port).or_default().push(info);
    }

    pub fn allocate_port(&mut self, range: Range<u16>, info: String) -> Result<u16> {
        for port in range {
            if !self.is_port_assigned(port) {
                self.add_port_info(port, info.to_string());
                return Ok(port);
            }
        }
        anyhow::bail!("No available ports in the given range")
    }

    /// Finds the smallest multiple of the offset that, when added to the base ports,
    /// results in a set of available ports within the specified range.
    ///
    /// The function iteratively checks port sets by adding multiples of the offset to the base ports:
    /// base, base + offset, base + 2*offset, etc., until it finds a set where all ports are:
    /// 1) Within the specified range
    /// 2) Not already assigned
    pub fn find_offset(
        &mut self,
        base_ports: &[u16],
        range: Range<u16>,
        offset: u16,
    ) -> Result<u16> {
        let mut i = 0;
        loop {
            let candidate_ports: Vec<u16> =
                base_ports.iter().map(|&port| port + i * offset).collect();

            // Check if any of the candidate ports ran beyond the upper range
            if candidate_ports.iter().any(|&port| port >= range.end) {
                anyhow::bail!(
                    "No suitable ports found within the given range {:?}. Tried {} iterations with offset {}",
                    range,
                    i,
                    offset
                )
            }

            // Check if all candidate ports are within the range and not assigned
            if candidate_ports
                .iter()
                .all(|&port| range.contains(&port) && !self.is_port_assigned(port))
            {
                return Ok(offset * i);
            }
            i += 1;
        }
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
        let mut dirs = vec![ecosystem_config.config.clone()];
        for chain in ecosystem_config.list_of_chains() {
            if let Some(chain_config) = ecosystem_config.load_chain(Some(chain)) {
                dirs.push(chain_config.configs.clone())
            }
        }

        let mut ecosystem_ports = EcosystemPorts::with_default_ports();
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

                    if key.as_str().map(|s| s.ends_with("port")).unwrap_or(false) {
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
    fn test_allocate_ports() {
        let mut ecosystem_ports = EcosystemPorts::default();

        // Test case 1: Allocate ports within range
        let result = ecosystem_ports.find_offset(&[8000, 8001, 8002], 8000..9000, 100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        // Test case 2: Allocate ports with offset
        let result = ecosystem_ports.find_offset(&[8000, 8001, 8002], 8000..9000, 100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);

        // Test case 3: Fail to allocate ports - not available with offset
        let result = ecosystem_ports.find_offset(&[8000, 8001, 8002], 8000..8200, 100);
        assert!(result.is_err());

        // Test case 4: Fail to allocate ports - base ports outside range
        let result = ecosystem_ports.find_offset(&[9500, 9501, 9502], 8000..9000, 100);
        assert!(result.is_err());

        // Test case 5: Allocate consecutive ports
        let result = ecosystem_ports.find_offset(&[8000, 8001, 8002], 8000..9000, 1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);

        // Test case 6: Allocate ports at the edge of the range
        let result = ecosystem_ports.find_offset(&[8998, 8999], 8000..9000, 1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

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
        "#;

        let value = serde_yaml::from_str(yaml_content).unwrap();
        let mut ecosystem_ports = EcosystemPorts::default();
        let file_path: PathBuf = PathBuf::from("test_config.yaml");

        EcosystemPortsScanner::traverse_yaml(&value, "", &file_path, &mut ecosystem_ports);

        // Assigned ports:
        assert!(ecosystem_ports.is_port_assigned(3050));
        assert!(ecosystem_ports.is_port_assigned(3051));
        assert!(ecosystem_ports.is_port_assigned(3070));
        assert!(ecosystem_ports.is_port_assigned(3412));

        // Free ports:
        assert!(!ecosystem_ports.is_port_assigned(3150));
        assert!(!ecosystem_ports.is_port_assigned(3151));

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
