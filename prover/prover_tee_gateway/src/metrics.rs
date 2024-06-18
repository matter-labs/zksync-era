use vise::{Counter, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_tee_gateway")]
pub(crate) struct ProverTeeGatewayMetrics {
    #[metrics(labels = ["service_name"])]
    pub http_error: LabeledFamily<&'static str, Counter>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProverTeeGatewayMetrics> = vise::Global::new();
