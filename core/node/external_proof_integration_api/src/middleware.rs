use tokio::time::Instant;

use crate::metrics::{CallOutcome, Method, METRICS};

#[derive(Debug)]
pub(crate) struct MetricsMiddleware {
    method: Method,
    started_at: Instant,
}

impl MetricsMiddleware {
    pub fn new(method: Method) -> MetricsMiddleware {
        MetricsMiddleware {
            method,
            started_at: Instant::now(),
        }
    }

    pub fn observe(&self, outcome: CallOutcome) {
        METRICS.call_latency[&(self.method, outcome)].observe(self.started_at.elapsed());
    }
}
