use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use common::docker::adjust_localhost_for_docker;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    apps::AppsChainExplorerConfig,
    consts::{EXPLORER_DOCKER_COMPOSE_FILE, LOCAL_CONFIGS_PATH, LOCAL_GENERATED_PATH},
    docker_compose::{DockerComposeConfig, DockerComposeService},
    traits::ZkToolboxConfig,
};

/// Explorer docker compose file. This file is auto-generated during "explorer" command
/// and is passed to Docker Compose to launch the explorer app and backend services.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerComposeConfig {
    #[serde(flatten)]
    pub docker_compose: DockerComposeConfig,
}

impl ZkToolboxConfig for ExplorerComposeConfig {}

/// Chain-level explorer backend docker compose config. It contains the configuration for
/// api, data fetcher, and worker services. This config is generated during "explorer" command
/// and serves as a building block for the main explorer docker compose file.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplorerBackendComposeConfig {
    #[serde(flatten)]
    pub docker_compose: DockerComposeConfig,
}

#[derive(Debug, Clone)]
pub struct ExplorerAppServiceConfig {
    pub port: u16,
    pub config_path: PathBuf,
}

impl ExplorerComposeConfig {
    pub fn new(
        app_config: ExplorerAppServiceConfig,
        backend_configs: Vec<ExplorerBackendComposeConfig>,
    ) -> anyhow::Result<Self> {
        let mut services = HashMap::new();
        let mut app_depends_on = Vec::new();

        // Add services from backend configs
        for backend_config in backend_configs.iter() {
            for (service_name, service) in &backend_config.docker_compose.services {
                if service.image.contains("block-explorer-api") {
                    app_depends_on.push(service_name.clone());
                }
                services.insert(service_name.clone(), service.clone());
            }
        }

        services.insert(
            "explorer-app".to_string(),
            Self::create_app_service(app_config, Some(app_depends_on)),
        );

        let config = Self {
            docker_compose: DockerComposeConfig { services },
        };
        Ok(config)
    }

    fn create_app_service(
        app_config: ExplorerAppServiceConfig,
        depends_on: Option<Vec<String>>,
    ) -> DockerComposeService {
        DockerComposeService {
            image: "matterlabs/block-explorer-app".to_string(),
            platform: Some("linux/amd64".to_string()),
            ports: Some(vec![format!("{}:3010", app_config.port)]),
            volumes: Some(vec![format!(
                "{}:/usr/src/app/packages/app/dist/config.js",
                app_config.config_path.display()
            )]),
            depends_on,
            restart: None,
            environment: None,
            extra_hosts: None,
        }
    }

    pub fn get_config_path(ecosystem_base_path: &Path) -> PathBuf {
        ecosystem_base_path
            .join(LOCAL_CONFIGS_PATH)
            .join(LOCAL_GENERATED_PATH)
            .join(EXPLORER_DOCKER_COMPOSE_FILE)
    }
}

impl ExplorerBackendComposeConfig {
    pub fn new(
        chain_name: &str,
        l2_rpc_url: Url,
        config: &AppsChainExplorerConfig,
    ) -> anyhow::Result<Self> {
        let db_url = adjust_localhost_for_docker(config.database_url.clone())?;
        let l2_rpc_url = adjust_localhost_for_docker(l2_rpc_url)?;

        // Parse database URL
        let db_host = db_url
            .host_str()
            .context("Failed to parse database URL: no host")?
            .to_string();
        let db_user = db_url.username().to_string();
        let db_password = db_url
            .password()
            .context("Failed to parse database URL: no password")?
            .to_string();
        let db_name = db_url.path().trim_start_matches('/').to_string();

        let mut services: HashMap<String, DockerComposeService> = HashMap::new();
        services.insert(
            format!("explorer-api-{}", chain_name),
            Self::create_api_service(
                chain_name,
                config.services.api_http_port,
                db_url.as_ref(),
            ),
        );
        services.insert(
            format!("explorer-data-fetcher-{}", chain_name),
            Self::create_data_fetcher_service(
                config.services.data_fetcher_http_port,
                l2_rpc_url.as_ref(),
            ),
        );
        services.insert(
            format!("explorer-worker-{}", chain_name),
            Self::create_worker_service(
                chain_name,
                config.services.worker_http_port,
                config.services.data_fetcher_http_port,
                l2_rpc_url.as_ref(),
                &db_host,
                &db_user,
                &db_password,
                &db_name,
                config.services.get_batches_processing_polling_interval(),
            ),
        );

        Ok(Self {
            docker_compose: DockerComposeConfig { services },
        })
    }

    fn create_api_service(chain_name: &str, port: u16, db_url: &str) -> DockerComposeService {
        DockerComposeService {
            image: "matterlabs/block-explorer-api".to_string(),
            platform: Some("linux/amd64".to_string()),
            ports: Some(vec![format!("{}:{}", port, port)]),
            volumes: None,
            depends_on: Some(vec![format!("explorer-worker-{}", chain_name)]),
            restart: None,
            environment: Some(HashMap::from([
                ("PORT".to_string(), port.to_string()),
                ("LOG_LEVEL".to_string(), "verbose".to_string()),
                ("NODE_ENV".to_string(), "development".to_string()),
                ("DATABASE_URL".to_string(), db_url.to_string()),
            ])),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
        }
    }

    fn create_data_fetcher_service(port: u16, l2_rpc_url: &str) -> DockerComposeService {
        DockerComposeService {
            image: "matterlabs/block-explorer-data-fetcher".to_string(),
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
        }
    }

    fn create_worker_service(
        chain_name: &str,
        port: u16,
        data_fetcher_port: u16,
        l2_rpc_url: &str,
        db_host: &str,
        db_user: &str,
        db_password: &str,
        db_name: &str,
        batches_processing_polling_interval: u64,
    ) -> DockerComposeService {
        let data_fetcher_url = format!(
            "http://explorer-data-fetcher-{}:{}",
            chain_name, data_fetcher_port
        );
        DockerComposeService {
            image: "matterlabs/block-explorer-worker".to_string(),
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
                ("DATABASE_USER".to_string(), db_user.to_string()),
                ("DATABASE_PASSWORD".to_string(), db_password.to_string()),
                ("DATABASE_NAME".to_string(), db_name.to_string()),
                ("BLOCKCHAIN_RPC_URL".to_string(), l2_rpc_url.to_string()),
                ("DATA_FETCHER_URL".to_string(), data_fetcher_url),
                (
                    "BATCHES_PROCESSING_POLLING_INTERVAL".to_string(),
                    batches_processing_polling_interval.to_string(),
                ),
            ])),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
        }
    }
}
