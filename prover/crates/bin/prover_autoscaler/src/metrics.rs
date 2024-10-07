use vise::{Counter, Gauge, LabeledFamily, Metrics};
use zksync_config::configs::prover_autoscaler::Gpu;

#[derive(Debug, Metrics)]
#[metrics(prefix = "autoscaler")]
pub(crate) struct AutoscalerMetrics {
    pub protocol_version: Gauge<u64>,
    pub calls: Counter<u64>,
    #[metrics(labels = ["target_cluster", "target_namespace", "gpu"])]
    pub provers: LabeledFamily<(String, String, Gpu), Gauge<u64>, 3>,
}

#[vise::register]
pub(crate) static AUTOSCALER_METRICS: vise::Global<AutoscalerMetrics> = vise::Global::new();
