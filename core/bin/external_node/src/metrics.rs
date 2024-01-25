use vise::{Gauge, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node")]
pub(crate) struct EnMetrics {
    #[metrics(labels = ["server_version", "protocol_version"])]
    pub version: LabeledFamily<(String, Option<u16>), Gauge<u64>, 2>,
}

#[vise::register]
pub(crate) static EN_METRICS: vise::Global<EnMetrics> = vise::Global::new();
