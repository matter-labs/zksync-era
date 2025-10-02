use vise::{Counter, EncodeLabelSet, Family, Gauge, LabeledFamily, Metrics};

use crate::{
    cluster_types::{ClusterName, DeploymentName, NamespaceName},
    key::Gpu,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct JobLabels {
    pub job: DeploymentName,
    pub target_cluster: ClusterName,
    pub target_namespace: NamespaceName,
    #[metrics(skip = Gpu::is_unknown)]
    pub gpu: Gpu,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "autoscaler")]
pub(crate) struct AutoscalerMetrics {
    #[metrics(labels = ["target_namespace", "protocol_version"])]
    pub prover_protocol_version: LabeledFamily<(NamespaceName, String), Gauge<usize>, 2>,
    #[metrics(labels = ["target_cluster", "target_namespace", "gpu"])]
    pub provers: LabeledFamily<(ClusterName, NamespaceName, Gpu), Gauge<usize>, 3>,
    pub jobs: Family<JobLabels, Gauge<usize>>,
    #[metrics(labels = ["agent_url"])]
    pub agent_not_ready: LabeledFamily<String, Counter, 1>,
    #[metrics(labels = ["target", "status"])]
    pub calls: LabeledFamily<(String, u16), Counter, 2>,
    #[metrics(labels = ["target_cluster"])]
    pub scale_errors: LabeledFamily<ClusterName, Gauge<u64>>,
    #[metrics(labels = ["target_namespace", "job"])]
    pub queue: LabeledFamily<(NamespaceName, DeploymentName), Gauge<usize>, 2>,
    #[metrics(labels = ["pod_name"])]
    pub stale_pods: LabeledFamily<String, Counter, 1>,
}

#[vise::register]
pub(crate) static AUTOSCALER_METRICS: vise::Global<AutoscalerMetrics> = vise::Global::new();
