//! High-level tests for the config system.

use std::path::Path;

use secrecy::ExposeSecret;

use crate::{
    configs::{
        da_client::avail::AvailClientConfig, object_store::ObjectStoreMode, wallets::Wallets,
        GeneralConfig, GenesisConfigWrapper, Secrets,
    },
    full_config_schema,
    sources::ConfigFilePaths,
    ContractsConfig, DAClientConfig,
};

#[test]
fn pre_smart_config_files_can_be_parsed() {
    let config_dir = Path::new("./src/tests/pre_smart_config");
    let paths = ConfigFilePaths {
        general: Some(config_dir.join("general.yaml")),
        secrets: Some(config_dir.join("secrets.yaml")),
        contracts: Some(config_dir.join("contracts.yaml")),
        genesis: Some(config_dir.join("genesis.yaml")),
        wallets: Some(config_dir.join("wallets.yaml")),
        consensus: None,
        external_node: None,
    };
    let config_sources = paths.into_config_sources(None).unwrap();
    let schema = full_config_schema();
    let mut repo = config_sources.build_repository(&schema);
    let general: GeneralConfig = repo.parse().unwrap();
    assert_general_config(general);
    let secrets: Secrets = repo.parse().unwrap();
    assert_secrets(secrets);
    repo.parse::<ContractsConfig>().unwrap();
    repo.parse::<GenesisConfigWrapper>().unwrap();
    repo.parse::<Wallets>().unwrap();
}

// These checks aren't intended to be exhaustive; they mostly check parsing completeness.
fn assert_general_config(general: GeneralConfig) {
    assert_eq!(general.api_config.unwrap().web3_json_rpc.http_port, 3050);

    let snapshot_recovery_store = general.snapshot_recovery.unwrap().object_store.unwrap();
    let ObjectStoreMode::FileBacked {
        file_backed_base_path,
    } = &snapshot_recovery_store.mode
    else {
        panic!("unexpected store: {snapshot_recovery_store:?}");
    };
    assert_eq!(file_backed_base_path.as_os_str(), "artifacts");
    assert_eq!(snapshot_recovery_store.max_retries, 100);

    let da_client = general.da_client_config.unwrap();
    let DAClientConfig::Avail(da_client) = &da_client else {
        panic!("unexpected DA config: {da_client:?}");
    };
    assert_eq!(
        da_client.bridge_api_url,
        "https://turing-bridge-api.avail.so"
    );
    let AvailClientConfig::FullClient(client) = &da_client.client else {
        panic!("unexpected DA config: {da_client:?}");
    };
    assert_eq!(client.app_id, 123_456);
    assert_eq!(
        client.seed_phrase.0.expose_secret(),
        "correct horse battery staple"
    );
}

// These checks aren't intended to be exhaustive; they mostly check parsing completeness.
fn assert_secrets(secrets: Secrets) {
    let server_url = secrets.postgres.server_url.unwrap();
    assert_eq!(
        server_url.expose_str(),
        "postgres://postgres:notsecurepassword@localhost/zksync_local"
    );
    secrets.postgres.prover_url.unwrap();

    let l1_rpc_url = secrets.l1.l1_rpc_url.unwrap();
    assert_eq!(l1_rpc_url.expose_str(), "http://127.0.0.1:8545/");
}
