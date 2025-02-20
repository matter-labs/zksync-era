use axum::http::StatusCode;
use tokio::time::Instant;

use crate::metrics::{Method, METRICS};

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

    pub fn observe(&self, status_code: StatusCode) {
        METRICS.call_latency[&(self.method, status_code.as_u16())]
            .observe(self.started_at.elapsed());
    }
}
