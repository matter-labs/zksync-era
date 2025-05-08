use assert_cmd::Command;
use chrono::{DateTime, Utc};
use circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use prover_cli::commands::status::utils::Status;
use zksync_prover_dal::{
    fri_witness_generator_dal::FriWitnessJobStatus, Connection, ConnectionPool, Prover, ProverDal,
};
use zksync_types::{
    basic_fri_types::AggregationRound,
    protocol_version::{L1VerifierConfig, ProtocolSemanticVersion},
    prover_dal::{
        ProofCompressionJobStatus, ProverJobStatus, ProverJobStatusFailed,
        ProverJobStatusInProgress, ProverJobStatusSuccessful, WitnessJobStatus,
        WitnessJobStatusSuccessful,
    },
    L1BatchId, L1BatchNumber, L2ChainId,
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

#[tokio::test]
#[doc = "prover_cli status batch -n 10000"]
async fn pli_status_of_non_existing_batch_succeeds() {
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
        .arg(connection_pool.database_url().expose_str())
        .arg("status")
        .arg("batch")
        .args(["-n", "10000"])
        .assert()
        .success()
        .stdout(NON_EXISTING_BATCH_STATUS_STDOUT);
}

#[tokio::test]
#[doc = "prover_cli status batch -n 10000 10001"]
async fn pli_status_of_multiple_non_existing_batch_succeeds() {
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
        .arg(connection_pool.database_url().expose_str())
        .arg("status")
        .arg("batch")
        .args(["-n", "10000", "10001"])
        .assert()
        .success()
        .stdout(MULTIPLE_NON_EXISTING_BATCHES_STATUS_STDOUT);
}

fn status_batch_0_expects(db_url: &str, expected_output: String) {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg(db_url)
        .arg("status")
        .arg("batch")
        .args(["-n", "0"])
        .assert()
        .success()
        .stdout(expected_output);
}

fn status_verbose_batch_0_expects(db_url: &str, expected_output: String) {
    Command::cargo_bin("prover_cli")
        .unwrap()
        .arg(db_url)
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
            L1BatchId::new(L2ChainId::zero(), batch_number),
            circuit_id as u8,
            0,
            sequence_number,
            aggregation_round,
            "",
            false,
            ProtocolSemanticVersion::default(),
            DateTime::<Utc>::default(),
        )
        .await
        .unwrap();
    connection
        .cli_test_dal()
        .update_prover_job(
            status,
            circuit_id as u8,
            aggregation_round as i64,
            batch_number,
            sequence_number,
        )
        .await;
}

async fn update_attempts_prover_job(
    status: ProverJobStatus,
    attempts: u8,
    circuit_id: BaseLayerCircuitType,
    aggregation_round: AggregationRound,
    batch_number: L1BatchNumber,
    sequence_number: usize,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .update_attempts_prover_job(
            status,
            attempts,
            circuit_id as u8,
            aggregation_round as i64,
            batch_number,
            sequence_number,
        )
        .await;
}

async fn update_attempts_lwg(
    status: ProverJobStatus,
    attempts: u8,
    circuit_id: BaseLayerCircuitType,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .update_attempts_lwg(status, attempts, circuit_id as u8, batch_number)
        .await;
}

async fn insert_bwg_job(
    status: FriWitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .fri_basic_witness_generator_dal()
        .save_witness_inputs(
            L1BatchId::new(L2ChainId::zero(), batch_number),
            "",
            ProtocolSemanticVersion::default(),
            DateTime::<Utc>::default(),
        )
        .await
        .unwrap();
    connection
        .fri_basic_witness_generator_dal()
        .set_status_for_basic_witness_job(status, L1BatchId::new(L2ChainId::zero(), batch_number))
        .await
        .unwrap();
}

async fn insert_lwg_job(
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    circuit_id: BaseLayerCircuitType,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .insert_lwg_job(status, batch_number, circuit_id as u8)
        .await;
}

async fn insert_nwg_job(
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    circuit_id: BaseLayerCircuitType,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .insert_nwg_job(status, batch_number, circuit_id as u8)
        .await;
}

async fn insert_rt_job(
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .insert_rt_job(status, batch_number)
        .await;
}

async fn insert_scheduler_job(
    status: WitnessJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .insert_scheduler_job(status, batch_number)
        .await;
}

async fn insert_compressor_job(
    status: ProofCompressionJobStatus,
    batch_number: L1BatchNumber,
    connection: &mut Connection<'_, Prover>,
) {
    connection
        .cli_test_dal()
        .insert_compressor_job(status, batch_number)
        .await;
}

#[derive(Default)]
struct Scenario {
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
}

impl Scenario {
    fn new(batch_number: L1BatchNumber) -> Scenario {
        Scenario {
            batch_number,
            ..Default::default()
        }
    }
    fn add_bwg(mut self, status: FriWitnessJobStatus) -> Self {
        self.bwg_status = Some(status);
        self
    }

    fn add_agg_0_prover_job(
        mut self,
        job_status: ProverJobStatus,
        circuit_type: BaseLayerCircuitType,
        sequence_number: usize,
    ) -> Self {
        if let Some(ref mut vec) = self.agg_0_prover_jobs_status {
            vec.push((job_status, circuit_type, sequence_number));
        } else {
            self.agg_0_prover_jobs_status = Some(vec![(job_status, circuit_type, sequence_number)]);
        }
        self
    }

    fn add_lwg(mut self, job_status: WitnessJobStatus, circuit_type: BaseLayerCircuitType) -> Self {
        if let Some(ref mut vec) = self.lwg_status {
            vec.push((job_status, circuit_type));
        } else {
            self.lwg_status = Some(vec![(job_status, circuit_type)]);
        }
        self
    }

    fn add_agg_1_prover_job(
        mut self,
        job_status: ProverJobStatus,
        circuit_type: BaseLayerCircuitType,
        sequence_number: usize,
    ) -> Self {
        if let Some(ref mut vec) = self.agg_1_prover_jobs_status {
            vec.push((job_status, circuit_type, sequence_number));
        } else {
            self.agg_1_prover_jobs_status = Some(vec![(job_status, circuit_type, sequence_number)]);
        }
        self
    }

    fn add_nwg(mut self, job_status: WitnessJobStatus, circuit_type: BaseLayerCircuitType) -> Self {
        if let Some(ref mut vec) = self.nwg_status {
            vec.push((job_status, circuit_type));
        } else {
            self.nwg_status = Some(vec![(job_status, circuit_type)]);
        }
        self
    }

    fn add_agg_2_prover_job(
        mut self,
        job_status: ProverJobStatus,
        circuit_type: BaseLayerCircuitType,
        sequence_number: usize,
    ) -> Self {
        if let Some(ref mut vec) = self.agg_2_prover_jobs_status {
            vec.push((job_status, circuit_type, sequence_number));
        } else {
            self.agg_2_prover_jobs_status = Some(vec![(job_status, circuit_type, sequence_number)]);
        }
        self
    }

    fn add_rt(mut self, status: WitnessJobStatus) -> Self {
        self.rt_status = Some(status);
        self
    }

    fn add_scheduler(mut self, status: WitnessJobStatus) -> Self {
        self.scheduler_status = Some(status);
        self
    }

    fn add_compressor(mut self, status: ProofCompressionJobStatus) -> Self {
        self.compressor_status = Some(status);
        self
    }
}

#[allow(clippy::too_many_arguments)]
async fn load_scenario(scenario: Scenario, connection: &mut Connection<'_, Prover>) {
    if let Some(status) = scenario.bwg_status {
        insert_bwg_job(status, scenario.batch_number, connection).await;
    }
    if let Some(jobs) = scenario.agg_0_prover_jobs_status {
        for (status, circuit_id, sequence_number) in jobs.into_iter() {
            insert_prover_job(
                status,
                circuit_id,
                AggregationRound::BasicCircuits,
                scenario.batch_number,
                sequence_number,
                connection,
            )
            .await;
        }
    }
    if let Some(jobs) = scenario.lwg_status {
        for (status, circuit_id) in jobs.into_iter() {
            insert_lwg_job(status, scenario.batch_number, circuit_id, connection).await;
        }
    }
    if let Some(jobs) = scenario.agg_1_prover_jobs_status {
        for (status, circuit_id, sequence_number) in jobs.into_iter() {
            insert_prover_job(
                status,
                circuit_id,
                AggregationRound::LeafAggregation,
                scenario.batch_number,
                sequence_number,
                connection,
            )
            .await;
        }
    }
    if let Some(jobs) = scenario.nwg_status {
        for (status, circuit_id) in jobs.into_iter() {
            insert_nwg_job(status, scenario.batch_number, circuit_id, connection).await;
        }
    }
    if let Some(jobs) = scenario.agg_2_prover_jobs_status {
        for (status, circuit_id, sequence_number) in jobs.into_iter() {
            insert_prover_job(
                status,
                circuit_id,
                AggregationRound::NodeAggregation,
                scenario.batch_number,
                sequence_number,
                connection,
            )
            .await;
        }
    }
    if let Some(status) = scenario.rt_status {
        insert_rt_job(status, scenario.batch_number, connection).await;
    }
    if let Some(status) = scenario.scheduler_status {
        insert_scheduler_job(status, scenario.batch_number, connection).await;
    }
    if let Some(status) = scenario.compressor_status {
        insert_compressor_job(status, scenario.batch_number, connection).await;
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
async fn pli_status_complete() {
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

    let batch_0 = L1BatchNumber(0);

    // A BWG is created for batch 0.
    let scenario = Scenario::new(batch_0).add_bwg(FriWitnessJobStatus::Queued);

    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // The BWS start, agg_round 0 prover jobs created. All WG set in wating for proofs.
    let scenario = Scenario::new(batch_0)
        .add_bwg(FriWitnessJobStatus::InProgress)
        .add_agg_0_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1)
        .add_agg_0_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 2)
        .add_agg_0_prover_job(
            ProverJobStatus::Queued,
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_lwg(WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM)
        .add_lwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_nwg(WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM)
        .add_nwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_rt(WitnessJobStatus::WaitingForProofs)
        .add_scheduler(WitnessJobStatus::WaitingForProofs);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // The BWS done, agg_round 0 prover jobs in progress.
    let scenario = Scenario::new(batch_0)
        .add_bwg(FriWitnessJobStatus::Successful)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::VM,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Queued,
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        );
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // Agg_round 0, prover jobs done for VM circuit, LWG set in queue.
    let scenario = Scenario::new(batch_0)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_lwg(WitnessJobStatus::Queued, BaseLayerCircuitType::VM);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // Agg_round 0: all prover jobs successful, LWG in progress. Agg_round 1: prover jobs in queue.
    let scenario = Scenario::new(batch_0)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_lwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
        )
        .add_lwg(
            WitnessJobStatus::InProgress,
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_agg_1_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1)
        .add_agg_1_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 2);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // LWG succees. Agg_round 1: Done for VM circuit.
    let scenario = Scenario::new(batch_0)
        .add_lwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            2,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        );
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // Agg_round 1: all prover jobs successful. NWG queue.
    let scenario = Scenario::new(batch_0)
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_nwg(WitnessJobStatus::Queued, BaseLayerCircuitType::VM)
        .add_nwg(
            WitnessJobStatus::Queued,
            BaseLayerCircuitType::DecommitmentsFilter,
        );
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // NWG successful for VM circuit, agg_round 2 prover jobs created.
    let scenario = Scenario::new(batch_0)
        .add_nwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
        )
        .add_nwg(
            WitnessJobStatus::InProgress,
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_agg_2_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // NWG successful, agg_round 2 prover jobs updated.
    let scenario = Scenario::new(batch_0)
        .add_nwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_agg_2_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_2_prover_job(
            ProverJobStatus::Queued,
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        );
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // Agg_round 2 prover jobs successful. RT in progress.
    let scenario = Scenario::new(batch_0)
        .add_agg_2_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_2_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_rt(WitnessJobStatus::InProgress);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // RT in successful, Scheduler in progress.
    let scenario = Scenario::new(batch_0)
        .add_rt(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        ))
        .add_scheduler(WitnessJobStatus::InProgress);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // Scheduler in successful, Compressor in progress.
    let scenario = Scenario::new(batch_0)
        .add_scheduler(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        ))
        .add_compressor(ProofCompressionJobStatus::InProgress);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        scenario_expected_stdout(
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
        ),
    );

    // Compressor Done.
    let scenario = Scenario::new(batch_0).add_compressor(ProofCompressionJobStatus::SentToServer);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        COMPLETE_BATCH_STATUS_STDOUT.into(),
    );
}

#[tokio::test]
async fn pli_status_complete_verbose() {
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

    let batch_0 = L1BatchNumber(0);

    let scenario = Scenario::new(batch_0)
        .add_bwg(FriWitnessJobStatus::Successful)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            3,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            3,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
            1,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::Decommiter,
            2,
        )
        .add_agg_0_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::Decommiter, 3)
        .add_agg_0_prover_job(
            ProverJobStatus::Queued,
            BaseLayerCircuitType::LogDemultiplexer,
            1,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Queued,
            BaseLayerCircuitType::LogDemultiplexer,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Queued,
            BaseLayerCircuitType::LogDemultiplexer,
            3,
        )
        .add_lwg(WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM)
        .add_lwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_lwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::Decommiter,
        )
        .add_lwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::LogDemultiplexer,
        )
        .add_nwg(WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM)
        .add_nwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_nwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::Decommiter,
        )
        .add_nwg(
            WitnessJobStatus::WaitingForProofs,
            BaseLayerCircuitType::LogDemultiplexer,
        )
        .add_rt(WitnessJobStatus::WaitingForProofs)
        .add_scheduler(WitnessJobStatus::WaitingForProofs);
    load_scenario(scenario, &mut connection).await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
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

    let scenario = Scenario::new(batch_0)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
            3,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::LogDemultiplexer,
            1,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::LogDemultiplexer,
            2,
        )
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::LogDemultiplexer,
            3,
        )
        .add_lwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
        )
        .add_lwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_lwg(
            WitnessJobStatus::InProgress,
            BaseLayerCircuitType::Decommiter,
        )
        .add_lwg(
            WitnessJobStatus::Queued,
            BaseLayerCircuitType::LogDemultiplexer,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            2,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            3,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            4,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            2,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            3,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::Decommiter,
            1,
        )
        .add_agg_1_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::Decommiter, 2)
        .add_agg_1_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::Decommiter,
            3,
        )
        .add_nwg(WitnessJobStatus::Queued, BaseLayerCircuitType::VM);
    load_scenario(scenario, &mut connection).await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
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
     - Total jobs: 3
     - Successful: 2
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

    let scenario = Scenario::new(batch_0)
        .add_lwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
        )
        .add_lwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::LogDemultiplexer,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            3,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
            1,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
            2,
        )
        .add_agg_1_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
            3,
        )
        .add_nwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
        )
        .add_nwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
        )
        .add_nwg(
            WitnessJobStatus::InProgress,
            BaseLayerCircuitType::Decommiter,
        )
        .add_nwg(
            WitnessJobStatus::Queued,
            BaseLayerCircuitType::LogDemultiplexer,
        )
        .add_agg_2_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_2_prover_job(
            ProverJobStatus::InProgress(ProverJobStatusInProgress::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        );
    load_scenario(scenario, &mut connection).await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
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

    let scenario = Scenario::new(batch_0)
        .add_nwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::Decommiter,
        )
        .add_nwg(
            WitnessJobStatus::Successful(WitnessJobStatusSuccessful::default()),
            BaseLayerCircuitType::LogDemultiplexer,
        )
        .add_agg_2_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::DecommitmentsFilter,
            1,
        )
        .add_rt(WitnessJobStatus::InProgress);
    load_scenario(scenario, &mut connection).await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
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

    let scenario = Scenario::new(batch_0)
        .add_rt(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        ))
        .add_scheduler(WitnessJobStatus::InProgress);
    load_scenario(scenario, &mut connection).await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
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

    let scenario = Scenario::new(batch_0)
        .add_scheduler(WitnessJobStatus::Successful(
            WitnessJobStatusSuccessful::default(),
        ))
        .add_compressor(ProofCompressionJobStatus::SentToServer);
    load_scenario(scenario, &mut connection).await;

    status_batch_0_expects(
        connection_pool.database_url().expose_str(),
        COMPLETE_BATCH_STATUS_STDOUT.into(),
    );
}

#[tokio::test]
async fn pli_status_stuck_job() {
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

    let batch_0 = L1BatchNumber(0);

    let scenario = Scenario::new(batch_0)
        .add_bwg(FriWitnessJobStatus::Successful)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            1,
        )
        .add_agg_0_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 2)
        .add_lwg(WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM)
        .add_nwg(WitnessJobStatus::WaitingForProofs, BaseLayerCircuitType::VM)
        .add_rt(WitnessJobStatus::WaitingForProofs)
        .add_scheduler(WitnessJobStatus::WaitingForProofs);
    load_scenario(scenario, &mut connection).await;

    update_attempts_prover_job(
        ProverJobStatus::Failed(ProverJobStatusFailed::default()),
        10,
        BaseLayerCircuitType::VM,
        AggregationRound::BasicCircuits,
        batch_0,
        2,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
v Prover Jobs: Stuck ⛔️
   > VM: Stuck ⛔️
     - Prover Job: 2 stuck after 10 attempts

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

    let scenario = Scenario::new(batch_0)
        .add_agg_0_prover_job(
            ProverJobStatus::Successful(ProverJobStatusSuccessful::default()),
            BaseLayerCircuitType::VM,
            2,
        )
        .add_lwg(WitnessJobStatus::InProgress, BaseLayerCircuitType::VM)
        .add_agg_1_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 1)
        .add_agg_1_prover_job(ProverJobStatus::Queued, BaseLayerCircuitType::VM, 2);
    load_scenario(scenario, &mut connection).await;

    update_attempts_lwg(
        ProverJobStatus::Failed(ProverJobStatusFailed::default()),
        10,
        BaseLayerCircuitType::VM,
        batch_0,
        &mut connection,
    )
    .await;

    status_verbose_batch_0_expects(
        connection_pool.database_url().expose_str(),
        "== Batch 0 Status ==

-- Aggregation Round 0 --
> Basic Witness Generator: Successful ✅
> Prover Jobs: Successful ✅

-- Aggregation Round 1 --
v Leaf Witness Generator: Stuck ⛔️
   > VM: Stuck ⛔️
v Prover Jobs: Queued 📥
   > VM: Queued 📥

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
}
