use vise::{Gauge, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "loadtest")]
pub(crate) struct LoadtestMetrics {
    #[metrics(labels = ["label"])]
    pub tps: LabeledFamily<String, Gauge<f64>>,
    pub master_account_balance: Gauge<f64>,
}

#[vise::register]
pub(crate) static LOADTEST_METRICS: vise::Global<LoadtestMetrics> = vise::Global::new();
