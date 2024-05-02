use assert_cmd::Command;
use prover_dal::{Connection, ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::Eip4844Blobs, protocol_version::L1VerifierConfig, L1BatchNumber,
    ProtocolVersionId,
};

const NON_EXISTING_BATCH_STATUS_STDOUT: &str = "== Batch 10000 Status ==

= Proving Stages =
-- Aggregation Round 0 --
Basic Witness Generator: Jobs not found 🚫

-- Aggregation Round 1 --
Leaf Witness Generator: Jobs not found 🚫

-- Aggregation Round 2 --
Node Witness Generator: Jobs not found 🚫

-- Aggregation Round 4 --
Recursion Tip: Jobs not found 🚫

-- Aggregation Round 4 --
Scheduler: Jobs not found 🚫

-- Compressor --
Jobs not found 🚫


";

const MULTIPLE_NON_EXISTING_BATCHES_STATUS_STDOUT: &str = "== Batch 10000 Status ==

= Proving Stages =
-- Aggregation Round 0 --
Basic Witness Generator: Jobs not found 🚫

-- Aggregation Round 1 --
Leaf Witness Generator: Jobs not found 🚫

-- Aggregation Round 2 --
Node Witness Generator: Jobs not found 🚫

-- Aggregation Round 4 --
Recursion Tip: Jobs not found 🚫

-- Aggregation Round 4 --
Scheduler: Jobs not found 🚫

-- Compressor --
Jobs not found 🚫


== Batch 10001 Status ==

= Proving Stages =
-- Aggregation Round 0 --
Basic Witness Generator: Jobs not found 🚫

-- Aggregation Round 1 --
Leaf Witness Generator: Jobs not found 🚫

-- Aggregation Round 2 --
Node Witness Generator: Jobs not found 🚫

-- Aggregation Round 4 --
Recursion Tip: Jobs not found 🚫

-- Aggregation Round 4 --
Scheduler: Jobs not found 🚫

-- Compressor --
Jobs not found 🚫


";

const BASIC_WITNESS_GENERATOR_QUEUED_BATCH_STATUS_STDOUT: &str = "== Batch 0 Status ==

= Proving Stages =
-- Aggregation Round 0 --
Basic Witness Generator: Queued 📥

-- Aggregation Round 1 --
Leaf Witness Generator: Jobs not found 🚫

-- Aggregation Round 2 --
Node Witness Generator: Jobs not found 🚫

-- Aggregation Round 4 --
Recursion Tip: Jobs not found 🚫

-- Aggregation Round 4 --
Scheduler: Jobs not found 🚫

-- Compressor --
Jobs not found 🚫


";

#[test]
#[doc = "prover_cli status"]
fn pli_status_empty_fails() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .assert()
        .failure();
}

#[test]
#[doc = "prover_cli status --help"]
fn pli_status_help_succeeds() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("help")
        .assert()
        .success();
}

#[test]
#[doc = "prover_cli status batch"]
fn pli_status_batch_empty_fails() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .assert()
        .failure();
}

#[test]
#[doc = "prover_cli status batch --help"]
fn pli_status_batch_help_succeeds() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .arg("--help")
        .assert()
        .success();
}

#[test]
#[doc = "prover_cli status batch -n 1"]
#[ignore = "this is flaky, if run locally it'd say 'Prover DB URL is absent', but if ran with zk f it succeeds"]
fn pli_status_batch_without_db_fails() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .args(["-n", "1"])
        .assert()
        .failure();
}

#[test]
#[doc = "prover_cli status batch -n 10000"]
fn pli_status_of_non_existing_batch_succeeds() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .args(["-n", "10000"])
        .assert()
        .success()
        .stdout(NON_EXISTING_BATCH_STATUS_STDOUT);
}

#[test]
#[doc = "prover_cli status batch -n 10000 10001"]
fn pli_status_of_multiple_non_existing_batch_succeeds() {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .args(["-n", "10000", "10001"])
        .assert()
        .success()
        .stdout(MULTIPLE_NON_EXISTING_BATCHES_STATUS_STDOUT);
}

async fn insert_witness_input(
    connection: &mut Connection<'_, Prover>,
    batch_number: L1BatchNumber,
) {
    connection
        .fri_witness_generator_dal()
        .save_witness_inputs(
            batch_number,
            "",
            ProtocolVersionId::default(),
            Eip4844Blobs::decode(&[0; 144]).unwrap(),
        )
        .await;
}

#[tokio::test]
async fn pli_status_batch_integration() {
    let connection_pool = ConnectionPool::<Prover>::test_pool().await;
    let mut connection = connection_pool.connection().await.unwrap();
    connection
        .fri_protocol_versions_dal()
        .save_prover_protocol_version(ProtocolVersionId::default(), L1VerifierConfig::default())
        .await;

    insert_witness_input(&mut connection, L1BatchNumber(0)).await;

    Command::cargo_bin("prover_cli")
        .unwrap()
        .env("DATABASE_PROVER_URL", connection_pool.database_url())
        .arg("status")
        .arg("batch")
        .args(["-n", "0"])
        .assert()
        .success()
        .stdout(BASIC_WITNESS_GENERATOR_QUEUED_BATCH_STATUS_STDOUT);

    // TODO: Continue
}
