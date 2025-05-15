use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{db, docker::adjust_localhost_for_docker};

use crate::{
    consts::{
        DEFAULT_EXPLORER_API_PORT, DEFAULT_EXPLORER_DATA_FETCHER_PORT,
        DEFAULT_EXPLORER_WORKER_PORT, EXPLORER_API_DOCKER_IMAGE,
        EXPLORER_DATA_FETCHER_DOCKER_IMAGE, EXPLORER_DOCKER_COMPOSE_FILE,
        EXPLORER_WORKER_DOCKER_IMAGE, LOCAL_CHAINS_PATH, LOCAL_CONFIGS_PATH,
    },
    docker_compose::{DockerComposeConfig, DockerComposeService},
    traits::ZkStackConfig,
    EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerBackendPorts {
    pub api_http_port: u16,
    pub data_fetcher_http_port: u16,
    pub worker_http_port: u16,
}

impl ExplorerBackendPorts {
    pub fn with_offset(&self, offset: u16) -> Self {
        ExplorerBackendPorts {
            api_http_port: self.api_http_port + offset,
            data_fetcher_http_port: self.data_fetcher_http_port + offset,
            worker_http_port: self.worker_http_port + offset,
        }
    }
}

impl Default for ExplorerBackendPorts {
    fn default() -> Self {
        ExplorerBackendPorts {
            api_http_port: DEFAULT_EXPLORER_API_PORT,
            data_fetcher_http_port: DEFAULT_EXPLORER_DATA_FETCHER_PORT,
            worker_http_port: DEFAULT_EXPLORER_WORKER_PORT,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerBackendConfig {
    pub database_url: Url,
    pub ports: ExplorerBackendPorts,
    pub batches_processing_polling_interval: u64,
}

impl ExplorerBackendConfig {
    pub fn new(database_url: Url, ports: &ExplorerBackendPorts) -> Self {
        ExplorerBackendConfig {
            database_url,
            ports: ports.clone(),
            batches_processing_polling_interval: EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL,
        }
    }
}

/// Chain-level explorer backend docker compose file.
/// It contains configuration for api, data fetcher, and worker services.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerBackendComposeConfig {
    #[serde(flatten)]
    pub docker_compose: DockerComposeConfig,
}

impl ZkStackConfig for ExplorerBackendComposeConfig {}

impl ExplorerBackendComposeConfig {
    const API_NAME: &'static str = "api";
    const DATA_FETCHER_NAME: &'static str = "data-fetcher";
    const WORKER_NAME: &'static str = "worker";

    pub fn new(
        chain_name: &str,
        l2_rpc_url: Url,
        config: &ExplorerBackendConfig,
    ) -> anyhow::Result<Self> {
        let db_url =
            adjust_localhost_for_docker(config.database_url.clone(), "host.docker.internal")?;
        let l2_rpc_url = adjust_localhost_for_docker(l2_rpc_url, "host.docker.internal")?;

        let mut services: HashMap<String, DockerComposeService> = HashMap::new();
        services.insert(
            Self::API_NAME.to_string(),
            Self::create_api_service(config.ports.api_http_port, db_url.as_ref()),
        );
        services.insert(
            Self::DATA_FETCHER_NAME.to_string(),
            Self::create_data_fetcher_service(
                config.ports.data_fetcher_http_port,
                l2_rpc_url.as_ref(),
            ),
        );

        let worker = Self::create_worker_service(
            config.ports.worker_http_port,
            config.ports.data_fetcher_http_port,
            l2_rpc_url.as_ref(),
            &db_url,
            config.batches_processing_polling_interval,
        )
        .context("Failed to create worker service")?;
        services.insert(Self::WORKER_NAME.to_string(), worker);

        Ok(Self {
            docker_compose: DockerComposeConfig {
                name: Some(format!("{chain_name}-explorer")),
                services,
                other: serde_json::Value::Null,
            },
        })
    }

    fn create_api_service(port: u16, db_url: &str) -> DockerComposeService {
        DockerComposeService {
            image: EXPLORER_API_DOCKER_IMAGE.to_string(),
            platform: Some("linux/amd64".to_string()),
            ports: Some(vec![format!("{}:{}", port, port)]),
            volumes: None,
            depends_on: Some(vec![Self::WORKER_NAME.to_string()]),
            restart: None,
            environment: Some(HashMap::from([
                ("PORT".to_string(), port.to_string()),
                ("LOG_LEVEL".to_string(), "verbose".to_string()),
                ("NODE_ENV".to_string(), "development".to_string()),
                ("DATABASE_URL".to_string(), db_url.to_string()),
            ])),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            other: serde_json::Value::Null,
        }
    }

    fn create_data_fetcher_service(port: u16, l2_rpc_url: &str) -> DockerComposeService {
        DockerComposeService {
            image: EXPLORER_DATA_FETCHER_DOCKER_IMAGE.to_string(),
            platform: Some("linux/amd64".to_string()),
            ports: Some(vec![format!("{}:{}", port, port)]),
            volumes: None,
            depends_on: None,
            restart: None,
            environment: Some(HashMap::from([
                ("PORT".to_string(), port.to_string()),
                ("LOG_LEVEL".to_string(), "verbose".to_string()),
                ("NODE_ENV".to_string(), "development".to_string()),
                ("BLOCKCHAIN_RPC_URL".to_string(), l2_rpc_url.to_string()),
            ])),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            other: serde_json::Value::Null,
        }
    }

    fn create_worker_service(
        port: u16,
        data_fetcher_port: u16,
        l2_rpc_url: &str,
        db_url: &Url,
        batches_processing_polling_interval: u64,
    ) -> anyhow::Result<DockerComposeService> {
        let data_fetcher_url = format!("http://{}:{}", Self::DATA_FETCHER_NAME, data_fetcher_port);

        // Parse database URL
        let db_config = db::DatabaseConfig::from_url(db_url)?;
        let db_user = db_url.username().to_string();
        let db_password = db_url.password().unwrap_or("");
        let db_port = db_url.port().unwrap_or(5432);
        let db_host = db_url
            .host_str()
            .context("Failed to parse database host")?
            .to_string();

        Ok(DockerComposeService {
            image: EXPLORER_WORKER_DOCKER_IMAGE.to_string(),
            platform: Some("linux/amd64".to_string()),
            ports: None,
            volumes: None,
            depends_on: None,
            restart: None,
            environment: Some(HashMap::from([
                ("PORT".to_string(), port.to_string()),
                ("LOG_LEVEL".to_string(), "verbose".to_string()),
                ("NODE_ENV".to_string(), "development".to_string()),
                ("DATABASE_HOST".to_string(), db_host.to_string()),
                ("DATABASE_PORT".to_string(), db_port.to_string()),
                ("DATABASE_USER".to_string(), db_user.to_string()),
                ("DATABASE_PASSWORD".to_string(), db_password.to_string()),
                ("DATABASE_NAME".to_string(), db_config.name.to_string()),
                ("BLOCKCHAIN_RPC_URL".to_string(), l2_rpc_url.to_string()),
                ("DATA_FETCHER_URL".to_string(), data_fetcher_url),
                (
                    "BATCHES_PROCESSING_POLLING_INTERVAL".to_string(),
                    batches_processing_polling_interval.to_string(),
                ),
            ])),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            other: serde_json::Value::Null,
        })
    }

    pub fn get_config_path(ecosystem_base_path: &Path, chain_name: &str) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CHAINS_PATH)
            .join(chain_name)
            .join(LOCAL_CONFIGS_PATH)
            .join(EXPLORER_DOCKER_COMPOSE_FILE)
    }
}
