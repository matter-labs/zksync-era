use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use common::{db, docker::adjust_localhost_for_docker};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    apps::AppsChainExplorerConfig,
    consts::{
        EXPLORER_API_DOCKER_IMAGE, EXPLORER_DATA_FETCHER_DOCKER_IMAGE,
        EXPLORER_DOCKER_COMPOSE_FILE, EXPLORER_WORKER_DOCKER_IMAGE, LOCAL_CHAINS_PATH,
        LOCAL_CONFIGS_PATH, LOCAL_GENERATED_PATH,
    },
    docker_compose::{DockerComposeConfig, DockerComposeService},
    traits::ZkToolboxConfig,
};

/// Chain-level explorer backend docker compose file. It contains the configuration for
/// api, data fetcher, and worker services.
/// This config is auto-generated during "explorer run-backend" command.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerBackendComposeConfig {
    #[serde(flatten)]
    pub docker_compose: DockerComposeConfig,
}

impl ZkToolboxConfig for ExplorerBackendComposeConfig {}

impl ExplorerBackendComposeConfig {
    pub fn new(
        chain_name: &str,
        l2_rpc_url: Url,
        config: &AppsChainExplorerConfig,
    ) -> anyhow::Result<Self> {
        let db_url = adjust_localhost_for_docker(config.database_url.clone())?;
        let l2_rpc_url = adjust_localhost_for_docker(l2_rpc_url)?;

        let mut services: HashMap<String, DockerComposeService> = HashMap::new();
        services.insert(
            Self::api_name(chain_name),
            Self::create_api_service(chain_name, config.services.api_http_port, db_url.as_ref()),
        );
        services.insert(
            Self::data_fetcher_name(chain_name),
            Self::create_data_fetcher_service(
                config.services.data_fetcher_http_port,
                l2_rpc_url.as_ref(),
            ),
        );

        let worker = Self::create_worker_service(
            chain_name,
            config.services.worker_http_port,
            config.services.data_fetcher_http_port,
            l2_rpc_url.as_ref(),
            &db_url,
            config.services.batches_processing_polling_interval,
        )
        .context("Failed to create worker service")?;
        services.insert(Self::worker_name(chain_name), worker);

        Ok(Self {
            docker_compose: DockerComposeConfig { services },
        })
    }

    fn create_api_service(chain_name: &str, port: u16, db_url: &str) -> DockerComposeService {
        DockerComposeService {
            image: EXPLORER_API_DOCKER_IMAGE.to_string(),
            platform: Some("linux/amd64".to_string()),
            ports: Some(vec![format!("{}:{}", port, port)]),
            volumes: None,
            depends_on: Some(vec![Self::worker_name(chain_name)]),
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
        chain_name: &str,
        port: u16,
        data_fetcher_port: u16,
        l2_rpc_url: &str,
        db_url: &Url,
        batches_processing_polling_interval: u64,
    ) -> anyhow::Result<DockerComposeService> {
        let data_fetcher_url = format!(
            "http://{}:{}",
            Self::data_fetcher_name(chain_name),
            data_fetcher_port
        );

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

    fn worker_name(chain_name: &str) -> String {
        format!("explorer-worker-{}", chain_name)
    }

    fn api_name(chain_name: &str) -> String {
        format!("explorer-api-{}", chain_name)
    }

    fn data_fetcher_name(chain_name: &str) -> String {
        format!("explorer-data-fetcher-{}", chain_name)
    }

    pub fn get_config_path(ecosystem_base_path: &Path, chain_name: &str) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CHAINS_PATH)
            .join(chain_name)
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_GENERATED_PATH)
            .join(EXPLORER_DOCKER_COMPOSE_FILE)
    }
}
