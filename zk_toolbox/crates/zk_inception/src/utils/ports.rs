use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde_yaml::Value;

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub file_name: String,
    pub key_path: String,
}

impl PortInfo {
    pub fn to_string(&self) -> String {
        format!("[{}] {}", self.file_name, self.key_path)
    }
}

pub struct EcosystemPorts {
    pub ports: HashMap<u16, Vec<PortInfo>>,
}

impl EcosystemPorts {
    pub fn new() -> Self {
        Self {
            ports: HashMap::new(),
        }
    }

    pub fn get_assigned_ports(&self) -> HashSet<u16> {
        self.ports.keys().cloned().collect()
    }

    pub fn get_port_info(&self, port: u16) -> Option<&Vec<PortInfo>> {
        self.ports.get(&port)
    }

    pub fn is_port_assigned(&self, port: u16) -> bool {
        self.ports.contains_key(&port)
    }

    pub fn add_port_info(&mut self, port: u16, info: PortInfo) {
        self.ports.entry(port).or_insert_with(Vec::new).push(info);
    }

    pub fn print(&self) {
        let mut port_vec: Vec<_> = self.ports.iter().collect();
        port_vec.sort_by_key(|&(key, _)| key);
        for (port, port_infos) in port_vec {
            for port_info in port_infos {
                println!("{} > {}", port_info.to_string(), port);
            }
        }
    }
}

pub struct EcosystemPortsScanner {}

impl EcosystemPortsScanner {
    /// Scans the ecosystem directory for YAML files and extracts port information
    /// Does not work with docker files
    pub fn scan(ecosystem_dir: &Path) -> Result<EcosystemPorts> {
        let skip_dirs: HashSet<&str> = vec!["db", "target", "volumes"].into_iter().collect();

        let mut ecosystem_ports = EcosystemPorts::new();

        let subdirs = ["chains", "configs"];
        for subdir in &subdirs {
            let dir = ecosystem_dir.join(subdir);
            if dir.is_dir() {
                if let Err(e) = Self::scan_directory(&dir, &mut ecosystem_ports, &skip_dirs) {
                    eprintln!("Error scanning directory {:?}: {}", dir, e);
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

        for entry in fs::read_dir(dir).context("Failed to read directory")? {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();
            if path.is_dir() {
                if let Err(e) = Self::scan_directory(&path, ecosystem_ports, skip_dirs) {
                    eprintln!("Error scanning directory {:?}: {}", path, e);
                }
            } else if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "yaml" || extension == "yml" {
                        if let Err(e) = Self::process_yaml_file(&path, ecosystem_ports) {
                            eprintln!("Error processing file {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn process_yaml_file(file_path: &Path, ecosystem_ports: &mut EcosystemPorts) -> Result<()> {
        let contents = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;
        let value: Value = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse YAML in file: {:?}", file_path))?;
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
                                PortInfo {
                                    file_name: file_path.to_string_lossy().into_owned(),
                                    key_path: new_path.clone(),
                                },
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
