use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, LabeledFamily, Metrics};
use zksync_types::{protocol_version::ProtocolSemanticVersion, L2ChainId};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_job_monitor")]
pub(crate) struct ProverJobMonitorMetrics {
    pub prover_job_archived: Counter,
    pub gpu_prover_archived: Counter,
    #[metrics(labels = ["job_type"])]
    pub reached_max_attempts: LabeledFamily<JobType, Gauge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum JobType {
    BasicWitnessGenerator,
    LeafWitnessGenerator,
    NodeWitnessGenerator,
    RecursionTipWitnessGenerator,
    SchedulerWitnessGenerator,
    ProverFri,
    ProofCompressor,
}

impl ProverJobMonitorMetrics {
    pub fn report_reached_max_attempts(&self, job_type: JobType, amount: usize) {
        PROVER_JOB_MONITOR_METRICS.reached_max_attempts[&job_type].set(amount as i64);
        if amount > 0 {
            tracing::warn!("{:?} jobs reached max attempts: {:?}", job_type, amount);
        }
    }
}

#[vise::register]
pub(crate) static PROVER_JOB_MONITOR_METRICS: vise::Global<ProverJobMonitorMetrics> =
    vise::Global::new();

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
        protocol_version: ProtocolSemanticVersion,
        amount: u64,
    ) {
        self.prover_jobs[&ProverJobsLabels {
            r#type,
            circuit_id: circuit_id.to_string(),
            aggregation_round: aggregation_round.to_string(),
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
    #[metrics(labels = ["chain_id"])]
    pub prover_fri_requeued_jobs: LabeledFamily<L2ChainId, Counter<u64>>,
    #[metrics(labels = ["witness_type", "chain_id"])]
    pub requeued_jobs: LabeledFamily<(WitnessType, L2ChainId), Counter<u64>, 2>,
    #[metrics(labels = ["type", "round", "protocol_version"])]
    pub witness_generator_jobs_by_round:
        LabeledFamily<(&'static str, String, String), Gauge<u64>, 3>,
    #[metrics(labels = ["type", "protocol_version", "chain_id"])]
    pub witness_generator_jobs: LabeledFamily<(&'static str, String, L2ChainId), Gauge<u64>, 3>,
    #[metrics(labels = ["witness_type", "chain_id"])]
    pub witness_generator_waiting_to_queued_jobs_transitions:
        LabeledFamily<(WitnessType, L2ChainId), Counter<u64>, 2>,
}

#[vise::register]
pub(crate) static SERVER_METRICS: vise::Global<ServerMetrics> = vise::Global::new();
