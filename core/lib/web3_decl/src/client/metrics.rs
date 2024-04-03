//! L2 client metrics.

use std::time::Duration;

use jsonrpsee::{core::client, http_client::transport};
use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};

use super::{AcquireStats, CallOrigin};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RequestLabels {
    pub component: &'static str,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RpcErrorLabels {
    pub component: &'static str,
    pub method: String,
    pub code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct HttpErrorLabels {
    pub component: &'static str,
    pub method: String,
    pub status: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(super) enum CallErrorKind {
    RequestTimeout,
    Parse,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct GenericErrorLabels {
    component: &'static str,
    method: String,
    kind: CallErrorKind,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "l2_client")]
pub(super) struct L2ClientMetrics {
    /// Number of requests timed out in the rate-limiting logic.
    pub rate_limit_timeout: Family<RequestLabels, Counter>,
    /// Latency of rate-limiting logic for rate-limited requests.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub rate_limit_latency: Family<RequestLabels, Histogram<Duration>>,
    /// Number of calls that resulted in an RPC-level error.
    pub rpc_errors: Family<RpcErrorLabels, Counter>,
    /// Number of calls that resulted in an HTTP-level error.
    pub http_errors: Family<HttpErrorLabels, Counter>,
    /// Number of calls that resulted in a generic / internal error.
    pub generic_errors: Family<GenericErrorLabels, Counter>,
}

impl L2ClientMetrics {
    pub fn observe_rate_limit_latency(
        &self,
        component: &'static str,
        origin: CallOrigin<'_>,
        stats: &AcquireStats,
    ) {
        for method in origin.distinct_method_names() {
            let request_labels = RequestLabels {
                component,
                method: method.to_owned(),
            };
            self.rate_limit_latency[&request_labels].observe(stats.total_sleep_time);
        }
    }

    pub fn observe_rate_limit_timeout(&self, component: &'static str, origin: CallOrigin<'_>) {
        for method in origin.distinct_method_names() {
            let request_labels = RequestLabels {
                component,
                method: method.to_owned(),
            };
            self.rate_limit_timeout[&request_labels].inc();
        }
    }

    pub fn observe_error(
        &self,
        component: &'static str,
        origin: CallOrigin<'_>,
        err: &client::Error,
    ) {
        for method in origin.distinct_method_names() {
            self.observe_error_inner(component, method, err);
        }
    }

    fn observe_error_inner(&self, component: &'static str, method: &str, err: &client::Error) {
        let kind = match err {
            client::Error::Call(err) => {
                let labels = RpcErrorLabels {
                    component,
                    method: method.to_owned(),
                    code: err.code(),
                };
                if self.rpc_errors[&labels].inc() == 0 {
                    tracing::warn!(
                        component,
                        method,
                        code = err.code(),
                        "Request `{method}` from component `{component}` failed with RPC error: {err}"
                    );
                }
                return;
            }

            client::Error::Transport(err) => {
                let status = err
                    .downcast_ref::<transport::Error>()
                    .and_then(|err| match err {
                        transport::Error::RequestFailure { status_code } => Some(*status_code),
                        _ => None,
                    });
                let labels = HttpErrorLabels {
                    component,
                    method: method.to_owned(),
                    status,
                };
                if self.http_errors[&labels].inc() == 0 {
                    tracing::warn!(
                        component,
                        method,
                        status,
                        "Request `{method}` from component `{component}` failed with HTTP error (response status: {status:?}): {err}"
                    );
                }
                return;
            }
            client::Error::RequestTimeout => CallErrorKind::RequestTimeout,
            client::Error::ParseError(_) => CallErrorKind::Parse,
            _ => CallErrorKind::Other,
        };

        let labels = GenericErrorLabels {
            component,
            method: method.to_owned(),
            kind,
        };
        if self.generic_errors[&labels].inc() == 0 {
            tracing::warn!(
                component,
                method,
                "Request `{method}` from component `{component}` failed with generic error: {err}"
            );
        }
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<L2ClientMetrics> = vise::Global::new();
