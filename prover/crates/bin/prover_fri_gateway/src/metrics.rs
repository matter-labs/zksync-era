use vise::{Counter, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_prover_fri_gateway")]
pub(crate) struct ProverFriGatewayMetrics {
    #[metrics(labels = ["service_name"])]
    pub http_error: LabeledFamily<&'static str, Counter>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProverFriGatewayMetrics> = vise::Global::new();
