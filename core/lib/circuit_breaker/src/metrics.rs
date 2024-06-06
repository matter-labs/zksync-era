//! Circuit breaker metrics.

use std::time::Duration;

use vise::{Gauge, Global, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "circuit_breaker")]
pub(crate) struct CircuitBreakerMetrics {
    /// Replication lag for Postgres in seconds.
    pub replication_lag: Gauge<Duration>,
}

#[vise::register]
pub(crate) static METRICS: Global<CircuitBreakerMetrics> = Global::new();
