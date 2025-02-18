use vise::{Counter, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_prover_fri_gateway")]
pub(crate) struct ProverFriGatewayMetrics {
    #[metrics(labels = ["status_code"])]
    pub submitter_http_error: LabeledFamily<u16, Counter>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProverFriGatewayMetrics> = vise::Global::new();
