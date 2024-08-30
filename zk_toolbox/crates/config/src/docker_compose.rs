use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

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

    pub fn adjust_host_for_docker(mut url: Url) -> anyhow::Result<Url> {
        if let Some(host) = url.host_str() {
            if Self::is_localhost(host) {
                url.set_host(Some("host.docker.internal"))?;
            }
        } else {
            anyhow::bail!("Failed to parse: no host");
        }
        Ok(url)
    }

    fn is_localhost(host: &str) -> bool {
        host == "localhost" || host == "127.0.0.1" || host == "[::1]"
    }
}
