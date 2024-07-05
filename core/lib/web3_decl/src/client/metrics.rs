//! L2 client metrics.

use std::time::Duration;

use jsonrpsee::{core::client, http_client::transport};
use vise::{
    Buckets, Counter, DurationAsSecs, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Info,
    LabeledFamily, Metrics, Unit,
};

use super::{AcquireStats, CallOrigin, SharedRateLimit};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RequestLabels {
    pub network: String,
    pub component: &'static str,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RpcErrorLabels {
    pub network: String,
    pub component: &'static str,
    pub method: String,
    pub code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct HttpErrorLabels {
    pub network: String,
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
    network: String,
    component: &'static str,
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
pub(super) struct L2ClientMetrics {
    /// Client configuration.
    #[metrics(labels = ["network"])]
    info: LabeledFamily<String, Info<L2ClientConfigLabels>>,
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

    pub fn observe_rate_limit_latency(
        &self,
        network: &str,
        component: &'static str,
        origin: CallOrigin<'_>,
        stats: &AcquireStats,
    ) {
        for method in origin.distinct_method_names() {
            let request_labels = RequestLabels {
                network: network.to_owned(),
                component,
                method: method.to_owned(),
            };
            self.rate_limit_latency[&request_labels].observe(stats.total_sleep_time);
        }
    }

    pub fn observe_rate_limit_timeout(
        &self,
        network: &str,
        component: &'static str,
        origin: CallOrigin<'_>,
    ) {
        for method in origin.distinct_method_names() {
            let request_labels = RequestLabels {
                network: network.to_owned(),
                component,
                method: method.to_owned(),
            };
            self.rate_limit_timeout[&request_labels].inc();
        }
    }

    pub fn observe_error(
        &self,
        network: &str,
        component: &'static str,
        origin: CallOrigin<'_>,
        err: &client::Error,
    ) {
        for method in origin.distinct_method_names() {
            self.observe_error_inner(network.to_owned(), component, method, err);
        }
    }

    fn observe_error_inner(
        &self,
        network: String,
        component: &'static str,
        method: &str,
        err: &client::Error,
    ) {
        let kind = match err {
            client::Error::Call(err) => {
                let labels = RpcErrorLabels {
                    network,
                    component,
                    method: method.to_owned(),
                    code: err.code(),
                };
                if self.rpc_errors[&labels].inc() == 0 {
                    tracing::warn!(
                        network = labels.network,
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
                        transport::Error::Rejected { status_code } => Some(*status_code),
                        _ => None,
                    });
                let labels = HttpErrorLabels {
                    network,
                    component,
                    method: method.to_owned(),
                    status,
                };
                if self.http_errors[&labels].inc() == 0 {
                    tracing::warn!(
                        network = labels.network,
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
            network,
            component,
            method: method.to_owned(),
            kind,
        };
        if self.generic_errors[&labels].inc() == 0 {
            tracing::warn!(
                network = labels.network,
                component,
                method,
                "Request `{method}` from component `{component}` failed with generic error: {err}",
            );
        }
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<L2ClientMetrics> = vise::Global::new();
