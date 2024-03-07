use std::time::Duration;
use vise::{Buckets, Histogram, LabeledFamily, Metrics, Unit};

#[derive(Debug, Metrics)]
#[metrics(prefix = "sql_connection")]
pub(crate) struct ConnectionMetrics {
    /// Lifetime of a DB connection, tagged with the requester label.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds, labels = ["requester"])]
    pub lifetime: LabeledFamily<&'static str, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static CONNECTION_METRICS: vise::Global<ConnectionMetrics> = vise::Global::new();
