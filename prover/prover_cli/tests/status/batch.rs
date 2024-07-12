use assert_cmd::Command;
use circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use prover_cli::commands::status::utils::Status;
use zksync_prover_dal::{
    fri_witness_generator_dal::FriWitnessJobStatus, Connection, ConnectionPool, Prover, ProverDal,
};
use zksync_types::{
    basic_fri_types::{AggregationRound, Eip4844Blobs},
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    prover_dal::{
        ProofCompressionJobStatus, ProverJobStatus, ProverJobStatusInProgress,
        ProverJobStatusSuccessful, WitnessJobStatus, WitnessJobStatusSuccessful,
    },
    L1BatchNumber,
};

const NON_EXISTING_BATCH_STATUS_STDOUT: &str = "== Batch 10000 Status ==
> No batch found. 🚫
";

const MULTIPLE_NON_EXISTING_BATCHES_STATUS_STDOUT: &str = "== Batch 10000 Status ==
> No batch found. 🚫
== Batch 10001 Status ==
> No batch found. 🚫
";

const COMPLETE_BATCH_STATUS_STDOUT: &str = "== Batch 0 Status ==
> Proof sent to server ✅
";

#[tokio::test]
#[doc = "prover_cli config"]
async fn pli_config_succeeds() {
    let connection_pool = ConnectionPool::<Prover>::test_pool().await;
    let mut connection = connection_pool.connection().await.unwrap();

    connection
        .fri_protocol_versions_dal()
        .save_prover_protocol_version(
            ProtocolSemanticVersion::default(),
            L1VerifierConfig::default(),
        )
        .await;

    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("config")
        .arg(connection_pool.database_url().expose_str())
        .assert()
        .success();
}

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

fn status_batch_0_expects(expected_output: String) {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .args(["-n", "0"])
        .assert()
        .success()
        .stdout(expected_output);
}

fn status_verbose_batch_0_expects(expected_output: String) {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("status")
        .arg("batch")
        .args(["-n", "0", "--verbose"])
        .assert()
        .success()
        .stdout(expected_output);
}

async fn insert_prover_job(
    status: ProverJobStatus,
    circuit_id: BaseLayerCircuitType,
    aggregation_round: AggregationRound,
    batch_number: L1BatchNumber,
    sequence_number: usize,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .fri_prover_jobs_dal()
        .insert_prover_job(
            batch_number,
            circuit_id as u8,
            0,
            sequence_number,
            aggregation_round,
            "",
            false,
            ProtocolSemanticVersion::default(),
        )
        .await;
    sqlx::query(&format!(
        "UPDATE prover_jobs_fri SET status = '{}' 
            WHERE l1_batch_number = {} 
            AND sequence_number = {} 
            AND aggregation_round = {}
            AND circuit_id = {}",
        status, batch_number.0, sequence_number, aggregation_round as i64, circuit_id as u8,
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
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    circuit_id: BaseLayerCircuitType,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            leaf_aggregation_witness_jobs_fri (
                l1_batch_number,
                circuit_id,
                status,
                number_of_basic_circuits,
                created_at,
                updated_at
            )
        VALUES
            ({}, {}, 'waiting_for_proofs', 2, NOW(), NOW())
        ON CONFLICT (l1_batch_number, circuit_id) DO
        UPDATE
        SET status = '{}'
        ",
        batch_number.0, circuit_id as u8, status
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_nwg_job(
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    circuit_id: BaseLayerCircuitType,
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
            ({}, {}, 'waiting_for_proofs', NOW(), NOW())
        ON CONFLICT (l1_batch_number, circuit_id, depth) DO
        UPDATE
        SET status = '{}'
        ",
        batch_number.0, circuit_id as u8, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_rt_job(
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    sqlx::query(&format!(
        "
        INSERT INTO
            recursion_tip_witness_jobs_fri (
                l1_batch_number,
                status,
                number_of_final_node_jobs,
                created_at,
                updated_at
            )
        VALUES
            ({}, 'waiting_for_proofs',1, NOW(), NOW())
        ON CONFLICT (l1_batch_number) DO
        UPDATE
        SET status = '{}'
        ",
        batch_number.0, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

async fn insert_scheduler_job(
    status: WitnessJobStatus,
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
            ({}, '', 'waiting_for_proofs', NOW(), NOW())
        ON CONFLICT (l1_batch_number) DO
        UPDATE
        SET status = '{}'
        ",
        batch_number.0, status,
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
        ON CONFLICT (l1_batch_number) DO
        UPDATE
        SET status = '{}'
        ",
        batch_number.0, status, status,
    ))
    .execute(connection.conn())
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn create_scenario(
    bwg_status: Option<FriWitnessJobStatus>,
    agg_0_prover_jobs_status: Option<Vec<(ProverJobStatus, BaseLayerCircuitType, usize)>>,
    lwg_status: Option<Vec<(WitnessJobStatus, BaseLayerCircuitType)>>,
    agg_1_prover_jobs_status: Option<Vec<(ProverJobStatus, BaseLayerCircuitType, usize)>>,
    nwg_status: Option<Vec<(WitnessJobStatus, BaseLayerCircuitType)>>,
    agg_2_prover_jobs_status: Option<Vec<(ProverJobStatus, BaseLayerCircuitType, usize)>>,
    rt_status: Option<WitnessJobStatus>,
    scheduler_status: Option<WitnessJobStatus>,
    compressor_status: Option<ProofCompressionJobStatus>,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    if let Some(status) = bwg_status {
        insert_bwg_job(status, batch_number, connection).await;
    }
    if let Some(jobs) = agg_0_prover_jobs_status {
        for (status, circuit_id, sequence_number) in jobs.into_iter() {
            insert_prover_job(
                status,
                circuit_id,
                AggregationRound::BasicCircuits,
                batch_number,
                sequence_number,
                connection,
            )
            .await;
        }
    }
    if let Some(jobs) = lwg_status {
        for (status, circuit_id) in jobs.into_iter() {
            insert_lwg_job(status, batch_number, circuit_id, connection).await;
        }
    }
    if let Some(jobs) = agg_1_prover_jobs_status {
        for (status, circuit_id, sequence_number) in jobs.into_iter() {
            insert_prover_job(
                status,
                circuit_id,
                AggregationRound::LeafAggregation,
                batch_number,
                sequence_number,
                connection,
            )
            .await;
        }
    }
    if let Some(jobs) = nwg_status {
        for (status, circuit_id) in jobs.into_iter() {
            insert_nwg_job(status, batch_number, circuit_id, connection).await;
        }
    }
    if let Some(jobs) = agg_2_prover_jobs_status {
        for (status, circuit_id, sequence_number) in jobs.into_iter() {
            insert_prover_job(
                status,
                circuit_id,
                AggregationRound::NodeAggregation,
                batch_number,
                sequence_number,
                connection,
            )
            .await;
        }
    }
    if let Some(status) = rt_status {
        insert_rt_job(status, batch_number, connection).await;
    }
    if let Some(status) = scheduler_status {
        insert_scheduler_job(status, batch_number, connection).await;
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
    scheduler_status: Status,
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

    format!(
        "== Batch {} Status ==

-- Aggregation Round 0 --
Basic Witness Generator: {}{}

-- Aggregation Round 1 --
Leaf Witness Generator: {}{}

-- Aggregation Round 2 --
Node Witness Generator: {}{}

-- Aggregation Round 3 --
Recursion Tip: {}

-- Aggregation Round 4 --
Scheduler: {}

-- Proof Compression --
Compressor: {}
",
        batch_number.0,
        bwg_status,
        agg_0_prover_jobs_status,
        lwg_status,
        agg_1_prover_jobs_status,
        nwg_status,
        agg_2_prover_jobs_status,
        rt_status,
        scheduler_status,
        compressor_status
    )
}

#[tokio::test]
async fn basic_batch_status() {
    let connection_pool = ConnectionPool::<Prover>::test_pool().await;
    let mut connection = connection_pool.connection().await.unwrap();

    connection
        .fri_protocol_versions_dal()
        .save_prover_protocol_version(
            ProtocolSemanticVersion::default(),
            L1VerifierConfig::default(),
        )
        .await;

    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("config")
        .arg(connection_pool.database_url().expose_str())
        .assert()
        .success();

    let batch_0 = L1BatchNumber(0);

    // A BWG is created for batch 0.
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
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Queued,
        None,
        Status::JobsNotFound,
        None,
        Status::JobsNotFound,
        None,
        Status::JobsNotFound,
        Status::JobsNotFound,
        Status::JobsNotFound,
        batch_0,
    ));

    // The BWS start, agg_round 0 prover jobs created. All WG set in wating for proofs.
    create_scenario(
        Some(FriWitnessJobStatus::InProgress),
        Some(vec![
            (ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1),
            (ProverJobStatus::Queued, BaseLayerCircuitType::VM, 2),
            (
                ProverJobStatus::Queued,
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
        Some(vec![
            (WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
        ]),
        None,
        Some(vec![
            (WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
        ]),
        None,
        Some(WitnessJobStatus::WaitingForProofs),
        Some(WitnessJobStatus::WaitingForProofs),
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::InProgress,
        Some(Status::Queued),
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // The BWS done, agg_round 0 prover jobs in progress.
    create_scenario(
        Some(FriWitnessJobStatus::Successful),
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::VM,
                2,
            ),
            (
                ProverJobStatus::Queued,
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
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

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::InProgress),
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // Agg_round 0, prover jobs done for VM circuit, LWG set in queue.
    create_scenario(
        None,
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                2,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
        Some(vec![(WitnessJobStatus::Queued, BaseLayerCircuitType::VM)]),
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

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::InProgress),
        Status::Queued,
        None,
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // Agg_round 0: all prover jobs successful, LWG in progress. Agg_round 1: prover jobs in queue.
    create_scenario(
        None,
        Some(vec![(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )]),
        Some(vec![
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
            ),
            (
                WitnessJobStatus::InProgress,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
        ]),
        Some(vec![
            (ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1),
            (ProverJobStatus::Queued, BaseLayerCircuitType::VM, 2),
        ]),
        None,
        None,
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::InProgress,
        Some(Status::Queued),
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // LWG succees. Agg_round 1: Done for VM circuit.
    create_scenario(
        None,
        None,
        Some(vec![(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
        )]),
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                2,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
        None,
        None,
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::InProgress),
        Status::WaitingForProofs,
        None,
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // Agg_round 1: all prover jobs successful. NWG queue.
    create_scenario(
        None,
        None,
        None,
        Some(vec![(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )]),
        Some(vec![
            (WitnessJobStatus::Queued, BaseLayerCircuitType::VM),
            (
                WitnessJobStatus::Queued,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
        ]),
        None,
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Queued,
        None,
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // NWG successful for VM circuit, agg_round 2 prover jobs created.
    create_scenario(
        None,
        None,
        None,
        None,
        Some(vec![
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
            ),
            (
                WitnessJobStatus::InProgress,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
        ]),
        Some(vec![(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1)]),
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::InProgress,
        Some(Status::Queued),
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // NWG successful, agg_round 2 prover jobs updated.
    create_scenario(
        None,
        None,
        None,
        None,
        Some(vec![(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
        )]),
        Some(vec![
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::Queued,
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::InProgress),
        Status::WaitingForProofs,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // Agg_round 2 prover jobs successful. RT in progress.
    create_scenario(
        None,
        None,
        None,
        None,
        None,
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
        Some(WitnessJobStatus::InProgress),
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::InProgress,
        Status::WaitingForProofs,
        Status::JobsNotFound,
        batch_0,
    ));

    // RT in successful, Scheduler in progress.
    create_scenario(
        None,
        None,
        None,
        None,
        None,
        None,
        Some(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        )),
        Some(WitnessJobStatus::InProgress),
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Status::InProgress,
        Status::JobsNotFound,
        batch_0,
    ));

    // Scheduler in successful, Compressor in progress.
    create_scenario(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        )),
        Some(ProofCompressionJobStatus::InProgress),
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(scenario_expected_stdout(
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Some(Status::Successful),
        Status::Successful,
        Status::Successful,
        Status::InProgress,
        batch_0,
    ));

    // Compressor Done.
    create_scenario(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(ProofCompressionJobStatus::SentToServer),
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(COMPLETE_BATCH_STATUS_STDOUT.into());
}

#[tokio::test]
async fn verbose_batch_status() {
    let connection_pool = ConnectionPool::<Prover>::test_pool().await;
    let mut connection = connection_pool.connection().await.unwrap();

    connection
        .fri_protocol_versions_dal()
        .save_prover_protocol_version(
            ProtocolSemanticVersion::default(),
            L1VerifierConfig::default(),
        )
        .await;

    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg("config")
        .arg(connection_pool.database_url().expose_str())
        .assert()
        .success();

    let batch_0 = L1BatchNumber(0);

    create_scenario(
        Some(FriWitnessJobStatus::Successful),
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                2,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                3,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                2,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                3,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
                1,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::Decommiter,
                2,
            ),
            (ProverJobStatus::Queued, BaseLayerCircuitType::Decommiter, 3),
            (
                ProverJobStatus::Queued,
                BaseLayerCircuitType::LogDemultiplexer,
                1,
            ),
            (
                ProverJobStatus::Queued,
                BaseLayerCircuitType::LogDemultiplexer,
                2,
            ),
            (
                ProverJobStatus::Queued,
                BaseLayerCircuitType::LogDemultiplexer,
                3,
            ),
        ]),
        Some(vec![
            (WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::Decommiter,
            ),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::LogDemultiplexer,
            ),
        ]),
        None,
        Some(vec![
            (WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::Decommiter,
            ),
            (
                WitnessJobStatus::WaitingForProofs,
                BaseLayerCircuitType::LogDemultiplexer,
            ),
        ]),
        None,
        Some(WitnessJobStatus::WaitingForProofs),
        Some(WitnessJobStatus::WaitingForProofs),
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
v Prover Jobs: In Progress ⌛️
   > VM: Successful ✅
   > DecommitmentsFilter: Successful ✅
   > Decommiter: In Progress ⌛️
     - Total jobs: 3
     - Successful: 1
     - In Progress: 1
     - Queued: 1
     - Failed: 0
   > LogDemultiplexer: Queued 📥

-- Aggregation Round 1 --
 > Leaf Witness Generator: Waiting for Proof ⏱️

-- Aggregation Round 2 --
 > Node Witness Generator: Waiting for Proof ⏱️

-- Aggregation Round 3 --
 > Recursion Tip: Waiting for Proof ⏱️

-- Aggregation Round 4 --
 > Scheduler: Waiting for Proof ⏱️

-- Proof Compression --
 > Compressor: Jobs not found 🚫
"
        .into(),
    );

    create_scenario(
        None,
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
                2,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
                3,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::LogDemultiplexer,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::LogDemultiplexer,
                2,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::LogDemultiplexer,
                3,
            ),
        ]),
        Some(vec![
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
            ),
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
            (
                WitnessJobStatus::InProgress,
                BaseLayerCircuitType::Decommiter,
            ),
            (
                WitnessJobStatus::Queued,
                BaseLayerCircuitType::LogDemultiplexer,
            ),
        ]),
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                2,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                3,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                4,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                2,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                2,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::Decommiter,
                1,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::Decommiter,
                3,
            ),
            (ProverJobStatus::Queued, BaseLayerCircuitType::Decommiter, 2),
        ]),
        Some(vec![(WitnessJobStatus::Queued, BaseLayerCircuitType::VM)]),
        None,
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 1 --
v Leaf Witness Generator: In Progress ⌛️
   > VM: Successful ✅
   > DecommitmentsFilter: Successful ✅
   > Decommiter: In Progress ⌛️
   > LogDemultiplexer: Queued 📥
v Prover Jobs: In Progress ⌛️
   > VM: Successful ✅
   > DecommitmentsFilter: In Progress ⌛️
     - Total jobs: 2
     - Successful: 1
     - In Progress: 1
     - Queued: 0
     - Failed: 0
   > Decommiter: In Progress ⌛️
     - Total jobs: 3
     - Successful: 0
     - In Progress: 2
     - Queued: 1
     - Failed: 0

-- Aggregation Round 2 --
 > Node Witness Generator: Queued 📥

-- Aggregation Round 3 --
 > Recursion Tip: Waiting for Proof ⏱️

-- Aggregation Round 4 --
 > Scheduler: Waiting for Proof ⏱️

-- Proof Compression --
 > Compressor: Jobs not found 🚫
"
        .into(),
    );

    create_scenario(
        None,
        None,
        Some(vec![
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
            ),
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::LogDemultiplexer,
            ),
        ]),
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                2,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
                1,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
                3,
            ),
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
                2,
            ),
        ]),
        Some(vec![
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
            ),
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
            ),
            (
                WitnessJobStatus::InProgress,
                BaseLayerCircuitType::Decommiter,
            ),
            (
                WitnessJobStatus::Queued,
                BaseLayerCircuitType::LogDemultiplexer,
            ),
        ]),
        Some(vec![
            (
                ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
                BaseLayerCircuitType::VM,
                1,
            ),
            (
                ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
                BaseLayerCircuitType::DecommitmentsFilter,
                1,
            ),
        ]),
        None,
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 1 --
> Leaf Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 2 --
v Node Witness Generator: In Progress ⌛️
   > VM: Successful ✅
   > DecommitmentsFilter: Successful ✅
   > Decommiter: In Progress ⌛️
   > LogDemultiplexer: Queued 📥
v Prover Jobs: In Progress ⌛️
   > VM: Successful ✅
   > DecommitmentsFilter: In Progress ⌛️
     - Total jobs: 1
     - Successful: 0
     - In Progress: 1
     - Queued: 0
     - Failed: 0

-- Aggregation Round 3 --
 > Recursion Tip: Waiting for Proof ⏱️

-- Aggregation Round 4 --
 > Scheduler: Waiting for Proof ⏱️

-- Proof Compression --
 > Compressor: Jobs not found 🚫
"
        .into(),
    );

    create_scenario(
        None,
        None,
        None,
        None,
        Some(vec![
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::Decommiter,
            ),
            (
                WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
                BaseLayerCircuitType::LogDemultiplexer,
            ),
        ]),
        Some(vec![(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )]),
        Some(WitnessJobStatus::InProgress),
        None,
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 1 --
> Leaf Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 2 --
> Node Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 3 --
v Recursion Tip: In Progress ⌛️

-- Aggregation Round 4 --
 > Scheduler: Waiting for Proof ⏱️

-- Proof Compression --
 > Compressor: Jobs not found 🚫
"
        .into(),
    );

    create_scenario(
        None,
        None,
        None,
        None,
        None,
        None,
        Some(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        )),
        Some(WitnessJobStatus::InProgress),
        None,
        batch_0,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 1 --
> Leaf Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 2 --
> Node Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 3 --
> Recursion Tip: Successful ✅

-- Aggregation Round 4 --
v Scheduler: In Progress ⌛️

-- Proof Compression --
 > Compressor: Jobs not found 🚫
"
        .into(),
    );

    create_scenario(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        )),
        Some(ProofCompressionJobStatus::SentToServer),
        batch_0,
        &mut connection,
    )
    .await;

    status_batch_0_expects(COMPLETE_BATCH_STATUS_STDOUT.into());
}
