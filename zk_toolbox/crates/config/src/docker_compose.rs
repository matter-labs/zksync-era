use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::traits::ZkToolboxConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerComposeConfig {
    pub services: HashMap<String, DockerComposeService>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerComposeService {
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_hosts: Option<Vec<String>>,
}

impl ZkToolboxConfig for DockerComposeConfig {}

impl DockerComposeConfig {
    pub fn new() -> Self {
        DockerComposeConfig {
            services: HashMap::new(),
        }
    }

    pub fn add_service(&mut self, name: &str, service: DockerComposeService) {
        self.services.insert(name.to_string(), service);
    }
}

impl Default for DockerComposeConfig {
    fn default() -> Self {
        Self::new()
    }
}