use assert_cmd::Command;
use prover_cli::commands::status::utils::Status;
use prover_dal::{
    fri_witness_generator_dal::FriWitnessJobStatus, Connection, ConnectionPool, Prover, ProverDal,
};
use zksync_types::{
    basic_fri_types::{AggregationRound, Eip4844Blobs},
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    prover_dal::{ProofCompressionJobStatus, ProverJobStatus},
    L1BatchNumber,
};

const NON_EXISTING_BATCH_STATUS_STDOUT: &str = "== Batch 10000 Status ==
> No batch found. 🚫
";

const MULTIPLE_NON_EXISTING_BATCHES_STATUS_STDOUT: &str = "== Batch 10000 Status ==
> No batch found. 🚫
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

fn status_batch_0_expects(expected_output: String, db_url: &str) {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .env("DATABASE_PROVER_URL", db_url)
        .arg("status")
        .arg("batch")
        .args(["-n", "0"])
        .assert()
        .success()
        .stdout(expected_output);
}

async fn insert_prover_job(
    status: ProverJobStatus,
    aggregation_round: AggregationRound,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .fri_prover_jobs_dal()
        .insert_prover_job(
            batch_number,
            0,
            0,
            0,
            aggregation_round,
            "",
            false,
            ProtocolSemanticVersion::default(),
        )
        .await;
    sqlx::query(&format!(
        "UPDATE prover_jobs_fri SET status = '{}' WHERE l1_batch_number = {}",
        status, batch_number.0
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_bwg_job(
    status: FriWitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .fri_witness_generator_dal()
        .save_witness_inputs(
            batch_number,
            "",
            ProtocolSemanticVersion::default(),
            Eip4844Blobs::decode(&[0; 144]).unwrap(),
        )
        .await;
    connection
        .fri_witness_generator_dal()
        .mark_witness_job(status, batch_number)
        .await;
}

async fn insert_lwg_job(
    status: FriWitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            leaf_aggregation_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                closed_form_inputs_blob_url,
                number_of_basic_circuits,
                protocol_version,
                status,
                created_at,
                updated_at
            )
        VALUES
            ({}, {}, '{}', {}, {}, 'waiting_for_proofs', NOW(), NOW())
        ",
        batch_number.0,
        0,
        "",
        0,
        ProtocolSemanticVersion::default(),
    ))
    .execute(connection.conn())
    .await
    .unwrap();

    sqlx::query(&format!(
        "UPDATE leaf_aggregation_witness_jobs_fri SET status = '{}' WHERE l1_batch_number = {}",
        status, batch_number.0
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_nwg_job(
    status: FriWitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            node_aggregation_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                status,
                created_at,
                updated_at
            )
        VALUES
            ({}, {}, '{}', NOW(), NOW())
        ",
        batch_number.0, 0, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();

    sqlx::query(&format!(
        "UPDATE node_aggregation_witness_jobs_fri SET status = '{}' WHERE l1_batch_number = {}",
        status, batch_number.0
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_rt_job(
    status: FriWitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            recursion_tip_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                status,
                created_at,
                updated_at
            )
        VALUES
            ({}, {}, '{}', NOW(), NOW())
        ",
        batch_number.0, 0, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();

    sqlx::query(&format!(
        "UPDATE recursion_tip_witness_jobs_fri SET status = '{}' WHERE l1_batch_number = {}",
        status, batch_number.0
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_scheduler_job(
    status: FriWitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            scheduler_witness_jobs_fri (
                l1_batch_number,
                scheduler_partial_input_blob_url,
                status,
                created_at,
                updated_at
            )
        VALUES
            ({}, {}, {}, '{}', NOW(), NOW())
        ",
        batch_number.0, "", 0, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();

    sqlx::query(&format!(
        "UPDATE scheduler_witness_jobs_fri SET status = '{}' WHERE l1_batch_number = {}",
        status, batch_number.0
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_compressor_job(
    status: ProofCompressionJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            proof_compression_jobs_fri (
                l1_batch_number,
                status,
                created_at,
                updated_at
            )
        VALUES
            ({}, '{}', NOW(), NOW())
        ",
        batch_number.0, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();

    sqlx::query(&format!(
        "UPDATE proof_compression_jobs_fri SET status = '{}' WHERE l1_batch_number = {}",
        status, batch_number.0
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn create_scenario(
    bwg_status: Option<FriWitnessJobStatus>,
    agg_0_prover_jobs_status: Option<ProverJobStatus>,
    lwg_status: Option<FriWitnessJobStatus>,
    agg_1_prover_jobs_status: Option<ProverJobStatus>,
    nwg_status: Option<FriWitnessJobStatus>,
    agg_2_prover_jobs_status: Option<ProverJobStatus>,
    rt_status: Option<FriWitnessJobStatus>,
    agg_3_prover_jobs_status: Option<ProverJobStatus>,
    scheduler_status: Option<FriWitnessJobStatus>,
    agg_4_prover_jobs_status: Option<ProverJobStatus>,
    compressor_status: Option<ProofCompressionJobStatus>,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    if let Some(status) = bwg_status {
        insert_bwg_job(status, batch_number, connection).await;
    }
    if let Some(status) = agg_0_prover_jobs_status {
        insert_prover_job(
            status,
            AggregationRound::BasicCircuits,
            batch_number,
            connection,
        )
        .await;
    }
    if let Some(status) = lwg_status {
        insert_lwg_job(status, batch_number, connection).await;
    }
    if let Some(status) = agg_1_prover_jobs_status {
        insert_prover_job(
            status,
            AggregationRound::LeafAggregation,
            batch_number,
            connection,
        )
        .await;
    }
    if let Some(status) = nwg_status {
        insert_nwg_job(status, batch_number, connection).await;
    }
    if let Some(status) = agg_2_prover_jobs_status {
        insert_prover_job(
            status,
            AggregationRound::NodeAggregation,
            batch_number,
            connection,
        )
        .await;
    }
    if let Some(status) = rt_status {
        insert_rt_job(status, batch_number, connection).await;
    }
    if let Some(status) = agg_3_prover_jobs_status {
        insert_prover_job(
            status,
            AggregationRound::RecursionTip,
            batch_number,
            connection,
        )
        .await;
    }
    if let Some(status) = scheduler_status {
        insert_scheduler_job(status, batch_number, connection).await;
    }
    if let Some(status) = agg_4_prover_jobs_status {
        insert_prover_job(
            status,
            AggregationRound::Scheduler,
            batch_number,
            connection,
        )
        .await;
    }
    if let Some(status) = compressor_status {
        insert_compressor_job(status, batch_number, connection).await;
    }
}

#[allow(clippy::too_many_arguments)]
fn scenario_expected_stdout(
    bwg_status: Status,
    agg_0_prover_jobs_status: Option<Status>,
    lwg_status: Status,
    agg_1_prover_jobs_status: Option<Status>,
    nwg_status: Status,
    agg_2_prover_jobs_status: Option<Status>,
    rt_status: Status,
    agg_3_prover_jobs_status: Option<Status>,
    scheduler_status: Status,
    agg_4_prover_jobs_status: Option<Status>,
    compressor_status: Status,
    batch_number: L1BatchNumber,
) -> String {
    let agg_0_prover_jobs_status = match agg_0_prover_jobs_status {
        Some(status) => format!("\n> Prover Jobs: {}", status),
        None => String::new(),
    };
    let agg_1_prover_jobs_status = match agg_1_prover_jobs_status {
        Some(status) => format!("\n> Prover Jobs: {}", status),
        None => String::new(),
    };
    let agg_2_prover_jobs_status = match agg_2_prover_jobs_status {
        Some(status) => format!("\n> Prover Jobs: {}", status),
        None => String::new(),
    };
    let agg_3_prover_jobs_status = match agg_3_prover_jobs_status {
        Some(status) => format!("\n> Prover Jobs: {}", status),
        None => String::new(),
    };
    let agg_4_prover_jobs_status = match agg_4_prover_jobs_status {
        Some(status) => format!("\n> Prover Jobs: {}", status),
        None => String::new(),
    };

    format!(
        "== Batch {} Status ==

= Proving Stages =
-- Aggregation Round 0 --
Basic Witness Generator: {}{}

-- Aggregation Round 1 --
Leaf Witness Generator: {}{}

-- Aggregation Round 2 --
Node Witness Generator: {}{}

-- Aggregation Round 4 --
Recursion Tip: {}{}

-- Aggregation Round 4 --
Scheduler: {}{}

-- Compressor --
{}


",
        batch_number.0,
        bwg_status,
        agg_0_prover_jobs_status,
        lwg_status,
        agg_1_prover_jobs_status,
        nwg_status,
        agg_2_prover_jobs_status,
        rt_status,
        agg_3_prover_jobs_status,
        scheduler_status,
        agg_4_prover_jobs_status,
        compressor_status
    )
}

#[tokio::test]
async fn testito() {
    let connection_pool = ConnectionPool::<Prover>::test_pool().await;
    let mut connection = connection_pool.connection().await.unwrap();

    connection
        .fri_protocol_versions_dal()
        .save_prover_protocol_version(
            ProtocolSemanticVersion::default(),
            L1VerifierConfig::default(),
        )
        .await;

    let batch_0 = L1BatchNumber(0);

    create_scenario(
        Some(FriWitnessJobStatus::Queued),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(
        scenario_expected_stdout(
            Status::Queued,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            batch_0,
        ),
        connection_pool.database_url().expose_str(),
    );

    create_scenario(
        Some(FriWitnessJobStatus::InProgress),
        Some(ProverJobStatus::Queued),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(
        scenario_expected_stdout(
            Status::InProgress,
            Some(Status::Queued),
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            None,
            Status::JobsNotFound,
            batch_0,
        ),
        connection_pool.database_url().expose_str(),
    );
}
