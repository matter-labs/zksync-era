use std::{
    collections::{HashMap, HashSet},
    fs,
    ops::Range,
    path::Path,
};

use anyhow::Result;
use common::logger;
use serde_yaml::Value;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub description: String,
}

impl PortInfo {
    pub fn new(description: String) -> Self {
        Self { description }
    }

    pub fn from_config(file_path: &Path, key_path: String) -> Self {
        Self {
            description: format!("[{}] {}", file_path.display(), key_path),
        }
    }

    pub fn dynamically_allocated() -> Self {
        Self::new("Dynamically allocated".to_string())
    }
}

pub struct EcosystemPorts {
    pub ports: HashMap<u16, Vec<PortInfo>>,
}

impl EcosystemPorts {
    pub fn new() -> Self {
        Self {
            ports: HashMap::from([
                (3000, vec![PortInfo::new("Observability".to_string())]),
                (
                    3052,
                    vec![PortInfo::new("External node gateway".to_string())],
                ),
            ]),
        }
    }

    pub fn get_assigned_ports(&self) -> HashSet<u16> {
        self.ports.keys().cloned().collect()
    }

    pub fn is_port_assigned(&self, port: u16) -> bool {
        self.ports.contains_key(&port)
    }

    pub fn add_port_info(&mut self, port: u16, info: PortInfo) {
        self.ports.entry(port).or_default().push(info);
    }

    pub fn allocate_port(&mut self, range: Range<u16>) -> Result<u16> {
        for port in range {
            if !self.is_port_assigned(port) {
                self.add_port_info(port, PortInfo::dynamically_allocated());
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
                anyhow::bail!("No suitable ports found within the given range");
            }

            // Check if all candidate ports are available
            if candidate_ports
                .iter()
                .all(|&port| !self.is_port_assigned(port))
            {
                // Allocate all ports
                for &port in &candidate_ports {
                    self.add_port_info(port, PortInfo::dynamically_allocated());
                }
                return Ok(candidate_ports);
            }
            i += 1;
        }
    }

    pub fn print(&self) {
        let mut port_vec: Vec<_> = self.ports.iter().collect();
        port_vec.sort_by_key(|&(key, _)| key);
        for (port, port_infos) in port_vec {
            for port_info in port_infos {
                println!("{} > {}", port_info.description, port);
            }
        }
    }
}

pub struct EcosystemPortsScanner {}

impl EcosystemPortsScanner {
    /// Scans the ecosystem directory for YAML files and extracts port information.
    /// Specifically, it looks for keys ending with "port" and collects their values.
    /// Note: Port information from Docker Compose files will not be picked up by this method.
    pub fn scan(ecosystem_dir: &Path) -> Result<EcosystemPorts> {
        let skip_dirs: HashSet<&str> = vec!["db", "volumes"].into_iter().collect();

        let mut ecosystem_ports = EcosystemPorts::new();

        let subdirs = ["chains", "configs"];
        for subdir in &subdirs {
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
                let _ = Self::scan_directory(&path, ecosystem_ports, skip_dirs);
            } else if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "yaml" || extension == "yml" {
                        let _ = Self::process_yaml_file(&path, ecosystem_ports);
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
                            ecosystem_ports.add_port_info(
                                port,
                                PortInfo::from_config(file_path, new_path.clone()),
                            );
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
