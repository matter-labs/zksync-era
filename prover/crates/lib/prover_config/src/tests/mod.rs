use std::path::Path;

use assert_matches::assert_matches;
use zksync_config::configs::object_store::ObjectStoreMode;

use super::*;

#[test]
fn pre_smart_config_files_can_be_parsed() {
    let config_dir = Path::new("./src/tests/pre_smart_config");

    let config_sources = CompleteProverConfig::config_sources_inner(
        Some(config_dir.join("general.yaml")),
        Some(config_dir.join("secrets.yaml")),
        false,
    )
    .unwrap();
    let schema = CompleteProverConfig::schema().unwrap();
    let mut repo = config_sources.build_repository(&schema);
    let config: CompleteProverConfig = repo.parse().unwrap();

    assert_eq!(config.postgres_config.max_connections, Some(100));
    let prover_url = config.postgres_config.prover_url.unwrap();
    assert!(
        prover_url.expose_str().contains("prover_local"),
        "{prover_url:?}"
    );

    assert_eq!(config.prometheus_config.listener_port, Some(3_314));

    let prover = config.prover_config.unwrap();
    assert_eq!(prover.setup_data_path.as_os_str(), "data/keys");
    assert_matches!(
        prover.prover_object_store.mode,
        ObjectStoreMode::FileBacked { .. }
    );

    let proof_compressor = config.proof_compressor_config.unwrap();
    assert_eq!(proof_compressor.compression_mode, 5);

    let prover_gateway = config.prover_gateway.unwrap();
    assert_eq!(prover_gateway.port, Some(3_322));

    let witness_generator = config.witness_generator_config.unwrap();
    assert_eq!(witness_generator.max_attempts, 10);

    let prover_job_monitor = config.prover_job_monitor_config.unwrap();
    assert_eq!(prover_job_monitor.http_port, 3_074);
}
