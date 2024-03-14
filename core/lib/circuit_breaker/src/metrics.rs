//! Circuit breaker metrics.

use vise::{Gauge, Global, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "circuit_breaker")]
pub(crate) struct CircuitBreakerMetrics {
    /// Replication lag for Postgres in seconds.
    pub replication_lag: Gauge<u64>,
}

#[vise::register]
pub(crate) static METRICS: Global<CircuitBreakerMetrics> = Global::new();
