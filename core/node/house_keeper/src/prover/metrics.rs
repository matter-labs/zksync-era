use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, LabeledFamily, Metrics};
use zksync_types::ProtocolVersionId;

#[derive(Debug, Metrics)]
#[metrics(prefix = "house_keeper")]
pub(crate) struct HouseKeeperMetrics {
    pub prover_job_archived: Counter,
    pub gpu_prover_archived: Counter,
}

#[vise::register]
pub(crate) static HOUSE_KEEPER_METRICS: vise::Global<HouseKeeperMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum JobStatus {
    Queued,
    InProgress,
    Successful,
    Failed,
    SentToServer,
    Skipped,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri")]
pub(crate) struct ProverFriMetrics {
    pub proof_compressor_requeued_jobs: Counter<u64>,
    #[metrics(labels = ["type", "protocol_version"])]
    pub proof_compressor_jobs: LabeledFamily<(JobStatus, String), Gauge<u64>, 2>,
    pub proof_compressor_oldest_uncompressed_batch: Gauge<u64>,
}

#[vise::register]
pub(crate) static PROVER_FRI_METRICS: vise::Global<ProverFriMetrics> = vise::Global::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct ProverJobsLabels {
    pub r#type: &'static str,
    pub circuit_id: String,
    pub aggregation_round: String,
    pub prover_group_id: String,
    pub protocol_version: String,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "fri_prover")]
pub(crate) struct FriProverMetrics {
    pub prover_jobs: Family<ProverJobsLabels, Gauge<u64>>,
    #[metrics(labels = ["circuit_id", "aggregation_round"])]
    pub block_number: LabeledFamily<(String, String), Gauge<u64>, 2>,
    pub oldest_unpicked_batch: Gauge<u64>,
    pub oldest_not_generated_batch: Gauge<u64>,
    #[metrics(labels = ["round"])]
    pub oldest_unprocessed_block_by_round: LabeledFamily<String, Gauge<u64>>,
}

impl FriProverMetrics {
    pub fn report_prover_jobs(
        &self,
        r#type: &'static str,
        circuit_id: u8,
        aggregation_round: u8,
        prover_group_id: u8,
        protocol_version: ProtocolVersionId,
        amount: u64,
    ) {
        self.prover_jobs[&ProverJobsLabels {
            r#type,
            circuit_id: circuit_id.to_string(),
            aggregation_round: aggregation_round.to_string(),
            prover_group_id: prover_group_id.to_string(),
            protocol_version: protocol_version.to_string(),
        }]
            .set(amount);
    }
}

#[vise::register]
pub(crate) static FRI_PROVER_METRICS: vise::Global<FriProverMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "type", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum WitnessType {
    WitnessInputsFri,
    LeafAggregationJobsFri,
    NodeAggregationJobsFri,
    RecursionTipJobsFri,
    SchedulerJobsFri,
}

impl From<&str> for WitnessType {
    fn from(s: &str) -> Self {
        match s {
            "witness_inputs_fri" => Self::WitnessInputsFri,
            "leaf_aggregations_jobs_fri" => Self::LeafAggregationJobsFri,
            "node_aggregations_jobs_fri" => Self::NodeAggregationJobsFri,
            "recursion_tip_jobs_fri" => Self::RecursionTipJobsFri,
            "scheduler_jobs_fri" => Self::SchedulerJobsFri,
            _ => panic!("Invalid witness type"),
        }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server")]
pub(crate) struct ServerMetrics {
    pub prover_fri_requeued_jobs: Counter<u64>,
    pub requeued_jobs: Family<WitnessType, Counter<u64>>,
    #[metrics(labels = ["type", "round"])]
    pub witness_generator_jobs_by_round: LabeledFamily<(&'static str, String), Gauge<u64>, 2>,
    #[metrics(labels = ["type", "protocol_version"])]
    pub witness_generator_jobs: LabeledFamily<(&'static str, String), Gauge<u64>, 2>,
    pub leaf_fri_witness_generator_waiting_to_queued_jobs_transitions: Counter<u64>,
    pub node_fri_witness_generator_waiting_to_queued_jobs_transitions: Counter<u64>,
    pub recursion_tip_witness_generator_waiting_to_queued_jobs_transitions: Counter<u64>,
    pub scheduler_witness_generator_waiting_to_queued_jobs_transitions: Counter<u64>,
}

#[vise::register]
pub(crate) static SERVER_METRICS: vise::Global<ServerMetrics> = vise::Global::new();
