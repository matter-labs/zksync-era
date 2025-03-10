use vise::{Counter, EncodeLabelSet, Family, Gauge, LabeledFamily, Metrics};

use crate::key::Gpu;

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct JobLabels {
    pub job: String,
    pub target_cluster: String,
    pub target_namespace: String,
    #[metrics(skip = Gpu::is_unknown)]
    pub gpu: Gpu,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "autoscaler")]
pub(crate) struct AutoscalerMetrics {
    #[metrics(labels = ["target_namespace", "protocol_version"])]
    pub prover_protocol_version: LabeledFamily<(String, String), Gauge<usize>, 2>,
    #[metrics(labels = ["target_cluster", "target_namespace", "gpu"])]
    pub provers: LabeledFamily<(String, String, Gpu), Gauge<u64>, 3>,
    pub jobs: Family<JobLabels, Gauge<u64>>,
    pub clusters_not_ready: Counter,
    #[metrics(labels = ["target", "status"])]
    pub calls: LabeledFamily<(String, u16), Counter, 2>,
    #[metrics(labels = ["target_cluster"])]
    pub scale_errors: LabeledFamily<String, Gauge<u64>>,
    #[metrics(labels = ["target_namespace", "job"])]
    pub queue: LabeledFamily<(String, String), Gauge<u64>, 2>,
}

#[vise::register]
pub(crate) static AUTOSCALER_METRICS: vise::Global<AutoscalerMetrics> = vise::Global::new();
