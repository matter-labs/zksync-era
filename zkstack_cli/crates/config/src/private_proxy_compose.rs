use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde_json::json;
use url::Url;

use crate::{
    consts::{LOCAL_CHAINS_PATH, LOCAL_CONFIGS_PATH},
    docker_compose::DockerComposeService,
    PRIVATE_RPC_DOCKER_COMPOSE_FILE,
};

pub fn get_private_rpc_docker_compose_path(
    ecosystem_base_path: &Path,
    chain_name: &str,
) -> PathBuf {
    ecosystem_base_path
        .join(LOCAL_CHAINS_PATH)
        .join(chain_name)
        .join(LOCAL_CONFIGS_PATH)
        .join(PRIVATE_RPC_DOCKER_COMPOSE_FILE)
}

pub async fn create_private_rpc_service(
    database_url: Url,
    port: u16,
    create_token_secret: &str,
    l2_rpc_url: Url,
    ecosystem_path: &Path,
    chain_name: &str,
) -> anyhow::Result<DockerComposeService> {

    let permissions_path = ecosystem_path
        .join("chains")
        .join(chain_name)
        .join("configs")
        .join("private-rpc-permissions.yaml");

    Ok(DockerComposeService {
        image: "private-rpc".to_string(),
        platform: Some("linux/amd64".to_string()),
        ports: Some(vec![format!("{}:{}", port, port)]),
        volumes: Some(vec![
            format!("{}:/app/private-rpc-permissions.yaml:ro", permissions_path.display()),
        ]),
        depends_on: None,
        restart: None,
        environment: Some(HashMap::from([
            ("DATABASE_URL".to_string(), database_url.to_string()),
            ("PORT".to_string(), port.to_string()),
            (
                "PERMISSIONS_YAML_PATH".to_string(),
                "/app/private-rpc-permissions.yaml".to_string(),
            ),
            ("TARGET_RPC".to_string(), l2_rpc_url.to_string()),
            (
                "CORS_ORIGIN".to_string(),
                "http://localhost:3010".to_string(),
            ),
            ("PERMISSIONS_HOT_RELOAD".to_string(), "true".to_string()),
            (
                "CREATE_TOKEN_SECRET".to_string(),
                create_token_secret.to_string(),
            ),
        ])),
        extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
        other: json!({"network_mode": "host"}),
    })
}
