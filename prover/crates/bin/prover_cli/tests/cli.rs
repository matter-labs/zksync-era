use assert_cmd::Command;
use zksync_dal::ConnectionPool;
use zksync_prover_dal::{Prover, ProverDal};
use zksync_types::protocol_version::{L1VerifierConfig, ProtocolSemanticVersion};

#[test]
#[doc = "prover_cli"]
fn pli_empty_fails() {
    Command::cargo_bin("prover_cli").unwrap().assert().failure();
}

#[test]
#[doc = "prover_cli"]
fn pli_help_succeeds() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("help")
        .assert()
        .success();
}

#[tokio::test]
#[doc = "prover_cli config"]
async fn pli_config_succeeds() {
    let connection_pool = ConnectionPool::<Prover>::prover_test_pool().await;
    let mut connection = connection_pool.connection().await.unwrap();

    connection
        .fri_protocol_versions_dal()
        .save_prover_protocol_version(
            ProtocolSemanticVersion::default(),
            L1VerifierConfig::default(),
        )
        .await
        .unwrap();

    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("config")
        .arg(connection_pool.database_url().expose_str())
        .assert()
        .success();
}
