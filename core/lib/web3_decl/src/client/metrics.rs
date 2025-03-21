//! L2 client metrics.

use std::time::Duration;

use jsonrpsee::{core::client, http_client::transport};
use vise::{
    Buckets, Counter, DurationAsSecs, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Info,
    LabeledFamily, Metrics, MetricsFamily, Unit,
};

use super::{AcquireStats, CallOrigin, SharedRateLimit};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct ClientLabels {
    pub network: String,
    pub component: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RpcErrorLabels {
    pub method: String,
    pub code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct HttpErrorLabels {
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
    method: String,
    kind: CallErrorKind,
}

#[derive(Debug, EncodeLabelSet)]
struct L2ClientConfigLabels {
    rate_limit: usize,
    #[metrics(unit = Unit::Seconds)]
    rate_limit_window: DurationAsSecs,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "l2_client")]
pub(super) struct L2ClientInfoMetrics {
    /// Client configuration.
    #[metrics(labels = ["network"])]
    info: LabeledFamily<String, Info<L2ClientConfigLabels>>,
}

impl L2ClientInfoMetrics {
    pub fn observe_config(&self, network: String, rate_limit: &SharedRateLimit) {
        let config_labels = L2ClientConfigLabels {
            rate_limit: rate_limit.rate_limit,
            rate_limit_window: rate_limit.rate_limit_window.into(),
        };
        let info = &self.info[&network];
        if let Err(err) = info.set(config_labels) {
            tracing::debug!(
                "Error setting configuration info {:?} for L2 client; already set to {:?}",
                err.into_inner(),
                info.get()
            );
        }
    }
}

#[vise::register]
pub(super) static INFO_METRICS: vise::Global<L2ClientInfoMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "l2_client")]
pub(super) struct L2ClientMetrics {
    /// Number of requests timed out in the rate-limiting logic.
    #[metrics(labels = ["method"])]
    pub rate_limit_timeout: LabeledFamily<String, Counter>,
    /// Latency of rate-limiting logic for rate-limited requests.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds, labels = ["method"])]
    pub rate_limit_latency: LabeledFamily<String, Histogram<Duration>>,
    /// Number of calls that resulted in an RPC-level error.
    pub rpc_errors: Family<RpcErrorLabels, Counter>,
    /// Number of calls that resulted in an HTTP-level error.
    pub http_errors: Family<HttpErrorLabels, Counter>,
    /// Number of calls that resulted in a generic / internal error.
    pub generic_errors: Family<GenericErrorLabels, Counter>,
}

impl L2ClientMetrics {
    pub fn observe_rate_limit_latency(&self, origin: CallOrigin<'_>, stats: &AcquireStats) {
        for method in origin.distinct_method_names() {
            self.rate_limit_latency[&method.to_owned()].observe(stats.total_sleep_time);
        }
    }

    pub fn observe_rate_limit_timeout(&self, origin: CallOrigin<'_>) {
        for method in origin.distinct_method_names() {
            self.rate_limit_timeout[&method.to_owned()].inc();
        }
    }

    pub fn observe_error(&self, origin: CallOrigin<'_>, err: &client::Error) {
        for method in origin.distinct_method_names() {
            self.observe_error_inner(method, err);
        }
    }

    fn observe_error_inner(&self, method: &str, err: &client::Error) {
        let kind = match err {
            client::Error::Call(err) => {
                let labels = RpcErrorLabels {
                    method: method.to_owned(),
                    code: err.code(),
                };
                if self.rpc_errors[&labels].inc() == 0 {
                    tracing::warn!(
                        method,
                        code = err.code(),
                        "Request `{method}` failed with RPC error: {err}"
                    );
                }
                return;
            }

            client::Error::Transport(err) => {
                let status = err
                    .downcast_ref::<transport::Error>()
                    .and_then(|err| match err {
                        transport::Error::Rejected { status_code } => Some(*status_code),
                        _ => None,
                    });
                let labels = HttpErrorLabels {
                    method: method.to_owned(),
                    status,
                };
                if self.http_errors[&labels].inc() == 0 {
                    tracing::warn!(
                        method,
                        status,
                        "Request `{method}` failed with HTTP error (response status: {status:?}): {err}"
                    );
                }
                return;
            }
            client::Error::RequestTimeout => CallErrorKind::RequestTimeout,
            client::Error::ParseError(_) => CallErrorKind::Parse,
            _ => CallErrorKind::Other,
        };

        let labels = GenericErrorLabels {
            method: method.to_owned(),
            kind,
        };
        if self.generic_errors[&labels].inc() == 0 {
            tracing::warn!(
                method,
                "Request `{method}` failed with generic error: {err}",
            );
        }
    }
}

#[vise::register]
pub(super) static METRICS: MetricsFamily<ClientLabels, L2ClientMetrics> = MetricsFamily::new();
