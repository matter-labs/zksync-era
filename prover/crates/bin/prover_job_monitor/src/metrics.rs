use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, LabeledFamily, Metrics};
use zksync_types::protocol_version::ProtocolSemanticVersion;

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_job_monitor")]
pub(crate) struct ProverJobMonitorMetrics {
    // archivers
    /// number of dead GPU provers archived
    pub archived_gpu_provers: Counter,
    /// number of finished prover job archived
    pub archived_prover_jobs: Counter,

    // job requeuers
    /// number of proof compressor jobs that have been requeued for execution
    pub requeued_proof_compressor_jobs: Counter<u64>,
    /// number of circuit prover jobs that have been requeued for execution
    pub requeued_circuit_prover_jobs: Counter<u64>,
    /// number of witness generator jobs that have been requeued for execution
    pub requeued_witness_generator_jobs: Family<WitnessType, Counter<u64>>,

    // queues reporters
    /// number of proof compressor jobs that are queued/in_progress per protocol version
    #[metrics(labels = ["type", "protocol_version"])]
    pub proof_compressor_jobs: LabeledFamily<(JobStatus, String), Gauge<u64>, 2>,
    /// the oldest batch that has not been compressed yet
    pub oldest_uncompressed_batch: Gauge<u64>,
    /// number of prover jobs per circuit, per round, per protocol version, per status
    /// Sets a specific value for a struct as follows:
    /// {
    ///     status: Queued,
    ///     circuit_id: 1,
    ///     round: 0,
    ///     group_id:
    ///     protocol_version: 0.24.2,
    /// }
    pub prover_jobs: Family<ProverJobsLabels, Gauge<u64>>,
    /// the oldest batch that has not been proven yet, per circuit id and aggregation round
    #[metrics(labels = ["circuit_id", "aggregation_round"])]
    pub oldest_unprocessed_batch: LabeledFamily<(String, String), Gauge<u64>, 2>,
    /// number of witness generator jobs per "round"
    #[metrics(labels = ["type", "round", "protocol_version"])]
    pub witness_generator_jobs_by_round: LabeledFamily<(JobStatus, String, String), Gauge<u64>, 3>,

    // witness job queuer
    /// number of jobs queued per type of witness generator
    pub queued_witness_generator_jobs: Family<WitnessType, Counter<u64>>,
}

impl ProverJobMonitorMetrics {
    pub fn report_prover_jobs(
        &self,
        status: JobStatus,
        circuit_id: u8,
        round: u8,
        group_id: u8,
        protocol_version: ProtocolSemanticVersion,
        amount: u64,
    ) {
        self.prover_jobs[&ProverJobsLabels {
            status,
            circuit_id: circuit_id.to_string(),
            round: round.to_string(),
            group_id: group_id.to_string(),
            protocol_version: protocol_version.to_string(),
        }]
            .set(amount);
    }
}
#[vise::register]
pub(crate) static PROVER_JOB_MONITOR_METRICS: vise::Global<ProverJobMonitorMetrics> =
    vise::Global::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct ProverJobsLabels {
    pub status: JobStatus,
    pub circuit_id: String,
    pub round: String,
    pub group_id: String,
    pub protocol_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "type", rename_all = "snake_case")]
// #[allow(clippy::enum_variant_names)]
pub(crate) enum WitnessType {
    BasicWitnessGenerator,
    LeafWitnessGenerator,
    NodeWitnessGenerator,
    RecursionTipWitnessGenerator,
    SchedulerWitnessGenerator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    InProgress,
}
