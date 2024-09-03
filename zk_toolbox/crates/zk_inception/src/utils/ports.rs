use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    ops::Range,
    path::Path,
};

use anyhow::Result;
use common::logger;
use config::EcosystemConfig;
use serde_yaml::Value;
use xshell::Shell;

/// Ecosystem-level directories to include in the scan
const INCLUDE_DIRS: &[&str] = &["chains", "configs"];
/// Skip directories with these names at all levels
const SKIP_DIRS: &[&str] = &["db", "volumes"];

pub struct EcosystemPorts {
    pub ports: HashMap<u16, Vec<String>>,
}

impl EcosystemPorts {
    pub fn new() -> Self {
        Self {
            ports: HashMap::from([
                (3000, vec!["Observability".to_string()]),
                (3052, vec!["External node gateway".to_string()]),
            ]),
        }
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

    pub fn allocate_port(&mut self, range: Range<u16>) -> Result<u16> {
        for port in range {
            if !self.is_port_assigned(port) {
                self.add_port_info(port, "Dynamically allocated".to_string());
                return Ok(port);
            }
        }
        anyhow::bail!("No available ports in the given range")
    }

    /// Allocates a set of ports based on given base ports, incrementing by offset if needed.
    /// Tries base ports first, then base + offset, base + 2*offset, etc., until finding
    /// available ports or exceeding the range. Returns allocated ports or an error if
    /// no suitable ports are found.
    pub fn allocate_ports(
        &mut self,
        base_ports: &[u16],
        range: Range<u16>,
        offset: u16,
    ) -> Result<Vec<u16>> {
        let mut i = 0;
        loop {
            let candidate_ports: Vec<u16> =
                base_ports.iter().map(|&port| port + i * offset).collect();

            // Check if all candidate ports are within the range
            if candidate_ports.iter().any(|&port| !range.contains(&port)) {
                anyhow::bail!(
                    "No suitable ports found within the given range {:?}. Tried {} iterations with offset {}",
                    range,
                    i,
                    offset
                )
            }

            // Check if all candidate ports are available
            if candidate_ports
                .iter()
                .all(|&port| !self.is_port_assigned(port))
            {
                // Allocate all ports
                for &port in &candidate_ports {
                    self.add_port_info(port, "Dynamically allocated".to_string());
                }
                return Ok(candidate_ports);
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
        let _ = EcosystemConfig::from_file(shell)?; // Find and validate the ecosystem directory
        let ecosystem_dir = shell.current_dir();

        let skip_dirs: HashSet<&str> = SKIP_DIRS.iter().cloned().collect();
        let mut ecosystem_ports = EcosystemPorts::new();
        for subdir in INCLUDE_DIRS {
            let dir = ecosystem_dir.join(subdir);
            if dir.is_dir() {
                if let Err(e) = Self::scan_directory(&dir, &mut ecosystem_ports, &skip_dirs) {
                    logger::warn(format!("Error scanning directory {:?}: {}", dir, e));
                }
            }
        }

        Ok(ecosystem_ports)
    }

    fn scan_directory(
        dir: &Path,
        ecosystem_ports: &mut EcosystemPorts,
        skip_dirs: &HashSet<&str>,
    ) -> Result<()> {
        if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
            if skip_dirs.contains(dir_name) {
                return Ok(());
            }
        }

        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                if let Err(e) = Self::scan_directory(&path, ecosystem_ports, skip_dirs) {
                    logger::warn(format!("Error scanning directory {:?}: {}", path, e));
                }
            } else if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "yaml" || extension == "yml" {
                        if let Err(e) = Self::process_yaml_file(&path, ecosystem_ports) {
                            logger::warn(format!("Error processing YAML file {:?}: {}", path, e));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn process_yaml_file(file_path: &Path, ecosystem_ports: &mut EcosystemPorts) -> Result<()> {
        let contents = fs::read_to_string(file_path)?;
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
        let mut ecosystem_ports = EcosystemPorts::new();

        // Test case 1: Allocate ports within range
        let result = ecosystem_ports.allocate_ports(&[8000, 8001, 8002], 8000..9000, 100);
        assert!(result.is_ok());
        let allocated_ports = result.unwrap();
        assert_eq!(allocated_ports, vec![8000, 8001, 8002]);

        // Test case 2: Allocate ports with offset
        let result = ecosystem_ports.allocate_ports(&[8000, 8001, 8002], 8000..9000, 100);
        assert!(result.is_ok());
        let allocated_ports = result.unwrap();
        assert_eq!(allocated_ports, vec![8100, 8101, 8102]);

        // Test case 3: Fail to allocate ports - not available with offset
        let result = ecosystem_ports.allocate_ports(&[8000, 8001, 8002], 8000..8200, 100);
        assert!(result.is_err());

        // Test case 4: Fail to allocate ports - base ports outside range
        let result = ecosystem_ports.allocate_ports(&[9500, 9501, 9502], 8000..9000, 100);
        assert!(result.is_err());

        // Test case 5: Allocate consecutive ports
        let result = ecosystem_ports.allocate_ports(&[8000, 8001, 8002], 8000..9000, 1);
        assert!(result.is_ok());
        let allocated_ports = result.unwrap();
        assert_eq!(allocated_ports, vec![8003, 8004, 8005]);

        // Test case 6: Allocate ports at the edge of the range
        let result = ecosystem_ports.allocate_ports(&[8998, 8999], 8000..9000, 1);
        assert!(result.is_ok());
        let allocated_ports = result.unwrap();
        assert_eq!(allocated_ports, vec![8998, 8999]);
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
        let mut ecosystem_ports = EcosystemPorts::new();
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
