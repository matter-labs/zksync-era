use vise::{Counter, Gauge, LabeledFamily, Metrics};
//use zksync_types::protocol_version::ProtocolSemanticVersion;

use crate::cluster_types::GPU;

#[derive(Debug, Metrics)]
#[metrics(prefix = "autoscaler")]
pub(crate) struct AutoscalerMetrics {
    pub protocol_version: Gauge<u64>,
    pub calls: Counter<u64>,
    #[metrics(labels = ["target_cluster", "target_namespace", "gpu"])]
    pub provers: LabeledFamily<(String, String, GPU), Gauge<u64>, 3>,
}

#[vise::register]
pub(crate) static AUTOSCALER_METRICS: vise::Global<AutoscalerMetrics> = vise::Global::new();
