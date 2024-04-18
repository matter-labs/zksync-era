use vise::{Counter, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "house_keeper")]
pub(crate) struct HouseKeeperMetrics {
    pub prover_job_archived: Counter,
    pub gpu_prover_archived: Counter,
}

#[vise::register]
pub(crate) static HOUSE_KEEPER_METRICS: vise::Global<HouseKeeperMetrics> = vise::Global::new();
