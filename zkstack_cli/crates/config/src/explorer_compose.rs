use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{
    db,
    docker::{self, adjust_localhost_for_docker},
};
use zksync_basic_types::L2ChainId;

use crate::{
    consts::{
        DEFAULT_EXPLORER_API_PORT, DEFAULT_EXPLORER_DATA_FETCHER_PORT,
        DEFAULT_EXPLORER_WORKER_PORT, EXPLORER_API_DOCKER_IMAGE,
        EXPLORER_DATA_FETCHER_DOCKER_IMAGE, EXPLORER_DOCKER_COMPOSE_FILE,
        EXPLORER_WORKER_DOCKER_IMAGE, LOCAL_CHAINS_PATH, LOCAL_CONFIGS_PATH,
    },
    docker_compose::{DockerComposeConfig, DockerComposeService},
    traits::FileConfigTrait,
    EXPLORER_API_DOCKER_IMAGE_TAG, EXPLORER_API_PRIVIDIUM_DOCKER_IMAGE_TAG,
    EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL, EXPLORER_DATA_FETCHER_DOCKER_IMAGE_TAG,
    EXPLORER_DATA_FETCHER_PRIVIDIUM_DOCKER_IMAGE_TAG, EXPLORER_WORKER_DOCKER_IMAGE_TAG,
    EXPLORER_WORKER_PRIVIDIUM_DOCKER_IMAGE_TAG,
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
    pub prividium: Option<PrividiumExplorerBackendConfig>,
}

impl ExplorerBackendConfig {
    pub fn new(
        database_url: Url,
        ports: &ExplorerBackendPorts,
        prividium: Option<PrividiumExplorerBackendConfig>,
    ) -> Self {
        ExplorerBackendConfig {
            database_url,
            ports: ports.clone(),
            batches_processing_polling_interval: EXPLORER_BATCHES_PROCESSING_POLLING_INTERVAL,
            prividium,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrividiumExplorerBackendConfig {
    pub private_rpc_url: Url,
    pub private_rpc_secret: String,
    pub chain_id: L2ChainId,
    pub session_max_age: u64,
    pub session_same_site: String,
}

impl PrividiumExplorerBackendConfig {
    pub fn new(
        private_rpc_url: Url,
        private_rpc_secret: String,
        chain_id: L2ChainId,
        session_max_age: u64,
        session_same_site: String,
    ) -> Self {
        PrividiumExplorerBackendConfig {
            private_rpc_url,
            private_rpc_secret,
            chain_id,
            session_max_age,
            session_same_site,
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

impl FileConfigTrait for ExplorerBackendComposeConfig {}

impl ExplorerBackendComposeConfig {
    const API_NAME: &'static str = "api";
    const DATA_FETCHER_NAME: &'static str = "data-fetcher";
    const WORKER_NAME: &'static str = "worker";

    pub fn new(
        chain_name: &str,
        l2_rpc_url: Url,
        config: &ExplorerBackendConfig,
    ) -> anyhow::Result<Self> {
        let db_url = adjust_localhost_for_docker(config.database_url.clone())?;
        let l2_rpc_url = adjust_localhost_for_docker(l2_rpc_url)?;

        let mut services: HashMap<String, DockerComposeService> = HashMap::new();
        services.insert(
            Self::API_NAME.to_string(),
            Self::create_api_service(
                config.ports.api_http_port,
                db_url.as_ref(),
                &config.prividium,
            )?,
        );
        services.insert(
            Self::DATA_FETCHER_NAME.to_string(),
            Self::create_data_fetcher_service(
                config.ports.data_fetcher_http_port,
                l2_rpc_url.as_ref(),
                &config.prividium,
            ),
        );

        let worker = Self::create_worker_service(
            config.ports.worker_http_port,
            config.ports.data_fetcher_http_port,
            l2_rpc_url.as_ref(),
            &db_url,
            config.batches_processing_polling_interval,
            &config.prividium,
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

    fn create_api_service(
        port: u16,
        db_url: &str,
        prividium_config: &Option<PrividiumExplorerBackendConfig>,
    ) -> anyhow::Result<DockerComposeService> {
        let is_prividium = prividium_config.is_some();

        let mut env = HashMap::new();
        env.insert("PORT".to_string(), port.to_string());
        env.insert("LOG_LEVEL".to_string(), "verbose".to_string());
        env.insert("NODE_ENV".to_string(), "development".to_string());
        env.insert("DATABASE_URL".to_string(), db_url.to_string());
        env.insert("PRIVIDIUM".to_string(), is_prividium.to_string());

        if let Some(config) = prividium_config {
            env.insert(
                "PRIVIDIUM_PRIVATE_RPC_URL".to_string(),
                adjust_localhost_for_docker(config.private_rpc_url.clone())?.to_string(),
            );
            env.insert(
                "PRIVIDIUM_PRIVATE_RPC_SECRET".to_string(),
                config.private_rpc_secret.to_string(),
            );
            env.insert(
                "PRIVIDIUM_CHAIN_ID".to_string(),
                config.chain_id.to_string(),
            );
            env.insert(
                "PRIVIDIUM_SESSION_MAX_AGE".to_string(),
                config.session_max_age.to_string(),
            );
            env.insert(
                "PRIVIDIUM_SESSION_SAME_SITE".to_string(),
                config.session_same_site.to_string(),
            );
            env.insert(
                "APP_URL".to_string(),
                "http://127.0.0.1:3010".to_string(), //FIXME
            );
            env.insert(
                "APP_HOSTNAME".to_string(),
                "127.0.0.1".to_string(), //FIXME
            );
            env.insert("NODE_ENV".to_string(), "development".to_string());
        }

        let docker_image_tag = if is_prividium {
            EXPLORER_API_PRIVIDIUM_DOCKER_IMAGE_TAG
        } else {
            EXPLORER_API_DOCKER_IMAGE_TAG
        };
        let docker_image = docker::get_image_with_tag(EXPLORER_API_DOCKER_IMAGE, docker_image_tag);

        Ok(DockerComposeService {
            image: docker_image,
            platform: Some("linux/amd64".to_string()),
            ports: Some(vec![format!("{}:{}", port, port)]),
            volumes: None,
            depends_on: Some(vec![Self::WORKER_NAME.to_string()]),
            restart: None,
            environment: Some(env),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: None,
            other: serde_json::Value::Null,
        })
    }

    fn create_data_fetcher_service(
        port: u16,
        l2_rpc_url: &str,
        prividium_config: &Option<PrividiumExplorerBackendConfig>,
    ) -> DockerComposeService {
        let docker_image_tag = if prividium_config.is_some() {
            EXPLORER_DATA_FETCHER_PRIVIDIUM_DOCKER_IMAGE_TAG
        } else {
            EXPLORER_DATA_FETCHER_DOCKER_IMAGE_TAG
        };
        let docker_image =
            docker::get_image_with_tag(EXPLORER_DATA_FETCHER_DOCKER_IMAGE, docker_image_tag);

        DockerComposeService {
            image: docker_image,
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
            network_mode: None,
            other: serde_json::Value::Null,
        }
    }

    fn create_worker_service(
        port: u16,
        data_fetcher_port: u16,
        l2_rpc_url: &str,
        db_url: &Url,
        batches_processing_polling_interval: u64,
        prividium_config: &Option<PrividiumExplorerBackendConfig>,
    ) -> anyhow::Result<DockerComposeService> {
        let data_fetcher_url = format!("http://{}:{}", Self::DATA_FETCHER_NAME, data_fetcher_port);
        let docker_image_tag = if prividium_config.is_some() {
            EXPLORER_WORKER_PRIVIDIUM_DOCKER_IMAGE_TAG
        } else {
            EXPLORER_WORKER_DOCKER_IMAGE_TAG
        };
        let docker_image =
            docker::get_image_with_tag(EXPLORER_WORKER_DOCKER_IMAGE, docker_image_tag);

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
            image: docker_image,
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
            network_mode: None,
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
