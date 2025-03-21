//! L2 JSON-RPC clients, currently used for interaction between external nodes (from the client side)
//! and the main node (from the server side).
//!
//! # Overview
//!
//! - [`Client`] is the main client implementation. It's parameterized by the transport (e.g., HTTP or WS),
//!   with HTTP being the default option.
//! - [`MockClient`] is a mock client useful for testing. Bear in mind that because of the client being generic,
//!   mock tooling is fairly low-level. Prefer defining a domain-specific wrapper trait for the client functionality and mock it
//!   where it's possible.
//! - [`BoxedL2Client`] is a generic client (essentially, a wrapper around a trait object). Use it for dependency injection
//!   instead of `L2Client`. Both `L2Client` and `MockL2Client` are convertible to `BoxedL2Client`.

use std::{
    any,
    collections::HashSet,
    fmt,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
        params::BatchRequestBuilder,
        traits::ToRpcParams,
    },
    http_client::{HttpClient, HttpClientBuilder},
    ws_client,
};
use serde::de::DeserializeOwned;
use tokio::time::Instant;
use zksync_types::url::SensitiveUrl;

use self::metrics::{L2ClientMetrics, METRICS};
pub use self::{
    boxed::{DynClient, ObjectSafeClient},
    mock::{MockClient, MockClientBuilder},
    network::{ForWeb3Network, Network, TaggedClient, L1, L2},
    shared::Shared,
};
use crate::client::metrics::{ClientLabels, INFO_METRICS};

mod boxed;
mod metrics;
mod mock;
mod network;
mod rustls;
mod shared;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
enum CallOrigin<'a> {
    Notification(&'a str),
    Request(&'a str),
    BatchRequest(&'a BatchRequestBuilder<'a>),
    Subscription(&'a str),
}

impl<'a> CallOrigin<'a> {
    fn request_count(self) -> usize {
        match self {
            Self::BatchRequest(batch) => batch.iter().count(),
            _ => 1,
        }
    }

    fn distinct_method_names(self) -> HashSet<&'a str> {
        match self {
            Self::Notification(name) | Self::Request(name) | Self::Subscription(name) => {
                HashSet::from([name])
            }
            Self::BatchRequest(batch) => batch.iter().map(|(name, _)| name).collect(),
        }
    }
}

impl fmt::Display for CallOrigin<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Notification(name) => write!(formatter, "notification `{name}`"),
            Self::Request(name) => write!(formatter, "request `{name}`"),
            Self::BatchRequest(req) => {
                let method_names: Vec<_> = req.iter().map(|(name, _)| name).collect();
                write!(formatter, "batch request {method_names:?}")
            }
            Self::Subscription(name) => write!(formatter, "subscription `{name}`"),
        }
    }
}

/// Trait encapsulating requirements for the client base.
pub trait ClientBase: ClientT + Clone + fmt::Debug + Send + Sync + 'static {}

impl<T: ClientT + Clone + fmt::Debug + Send + Sync + 'static> ClientBase for T {}

/// JSON-RPC client for the main node or Ethereum node with built-in middleware support.
///
/// The client should be used instead of `HttpClient` etc. A single instance of the client should be built
/// and shared among all tasks run by the node in order to correctly rate-limit requests.
//
// # Why not use `HttpClient` with custom Tower middleware?
//
// - `HttpClient` allows to set HTTP-level middleware, not an RPC-level one. The latter is more appropriate for rate limiting.
// - Observability (logging, metrics) is also easier with RPC-level middleware.
// - We might want to add other middleware layers that could be better implemented on the RPC level
//   (e.g., automated batching of requests issued in a quick succession).
// - Naming the HTTP middleware type is problematic if middleware is complex. Tower provides
//   [a boxed cloneable version of services](https://docs.rs/tower/latest/tower/util/struct.BoxCloneService.html),
//   but it doesn't fit (it's unconditionally `!Sync`, and `Sync` is required for `HttpClient<_>` to implement RPC traits).
#[derive(Clone)]
pub struct Client<Net, C = HttpClient> {
    inner: C,
    url: SensitiveUrl,
    rate_limit: SharedRateLimit,
    component_name: &'static str,
    metrics: &'static L2ClientMetrics,
    network: Net,
}

/// Client using the WebSocket transport.
pub type WsClient<Net> = Client<Net, Shared<ws_client::WsClient>>;

impl<Net: fmt::Debug, C: 'static> fmt::Debug for Client<Net, C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            // Do not expose the entire client `Debug` representation, which may (and in case of `HttpClient`, does)
            // include potentially sensitive URL and/or HTTP headers
            .field("inner", &any::type_name::<C>())
            .field("url", &self.url)
            .field("rate_limit", &self.rate_limit)
            .field("component_name", &self.component_name)
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

impl<Net: Network> Client<Net> {
    /// Creates an HTTP-backed client.
    pub fn http(url: SensitiveUrl) -> anyhow::Result<ClientBuilder<Net>> {
        crate::client::rustls::set_rustls_backend_if_required();

        let client = HttpClientBuilder::default().build(url.expose_str())?;
        Ok(ClientBuilder::new(client, url))
    }
}

impl<Net: Network> WsClient<Net> {
    /// Creates a WS-backed client.
    pub async fn ws(
        url: SensitiveUrl,
    ) -> anyhow::Result<ClientBuilder<Net, Shared<ws_client::WsClient>>> {
        crate::client::rustls::set_rustls_backend_if_required();

        let client = ws_client::WsClientBuilder::default()
            .build(url.expose_str())
            .await?;
        Ok(ClientBuilder::new(Shared::new(client), url))
    }
}

impl<Net: Network, C: ClientBase> Client<Net, C> {
    async fn limit_rate(&self, origin: CallOrigin<'_>) -> Result<(), Error> {
        const RATE_LIMIT_TIMEOUT: Duration = Duration::from_secs(10);

        let rate_limit_result = tokio::time::timeout(
            RATE_LIMIT_TIMEOUT,
            self.rate_limit.acquire(origin.request_count()),
        )
        .await;

        let network_label = self.network.metric_label();
        let stats = match rate_limit_result {
            Err(_) => {
                self.metrics.observe_rate_limit_timeout(origin);
                tracing::warn!(
                    network = network_label,
                    component = self.component_name,
                    %origin,
                    "Request to {origin} by component `{}` timed out during rate limiting using policy {} reqs/{:?}",
                    self.component_name,
                    self.rate_limit.rate_limit,
                    self.rate_limit.rate_limit_window
                );
                return Err(Error::RequestTimeout);
            }
            Ok(stats) if !stats.was_waiting => return Ok(()),
            Ok(stats) => stats,
        };

        self.metrics.observe_rate_limit_latency(origin, &stats);
        tracing::debug!(
            network = network_label,
            component = self.component_name,
            %origin,
            "Request to {origin} by component `{}` was rate-limited using policy {} reqs/{:?}: {stats:?}",
            self.component_name,
            self.rate_limit.rate_limit,
            self.rate_limit.rate_limit_window
        );
        Ok(())
    }

    fn inspect_call_result<T>(
        &self,
        origin: CallOrigin<'_>,
        call_result: Result<T, Error>,
    ) -> Result<T, Error> {
        if let Err(err) = &call_result {
            let network_label = self.network.metric_label();
            let span = tracing::warn_span!(
                "observe_error",
                network = network_label,
                component = self.component_name
            );
            span.in_scope(|| self.metrics.observe_error(origin, err));
        }
        call_result
    }
}

impl<Net: Network, C: ClientBase> ForWeb3Network for Client<Net, C> {
    type Net = Net;

    fn network(&self) -> Self::Net {
        self.network
    }

    fn component(&self) -> &'static str {
        self.component_name
    }
}

impl<Net: Network, C: ClientBase> TaggedClient for Client<Net, C> {
    fn set_component(&mut self, component_name: &'static str) {
        self.component_name = component_name;
        self.metrics = &METRICS[&ClientLabels {
            network: self.network.metric_label(),
            component: component_name,
        }];
    }
}

#[async_trait]
impl<Net: Network, C: ClientBase> ClientT for Client<Net, C> {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        let origin = CallOrigin::Notification(method);
        self.limit_rate(origin).await?;
        self.inspect_call_result(origin, self.inner.notification(method, params).await)
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        let origin = CallOrigin::Request(method);
        self.limit_rate(origin).await?;
        self.inspect_call_result(origin, self.inner.request(method, params).await)
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        let origin = CallOrigin::BatchRequest(&batch);
        self.limit_rate(origin).await?;
        self.inner.batch_request(batch).await
    }
}

#[async_trait]
impl<Net: Network, C: ClientBase + SubscriptionClientT> SubscriptionClientT for Client<Net, C> {
    async fn subscribe<'a, Notif, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        let origin = CallOrigin::Subscription(subscribe_method);
        self.limit_rate(origin).await?;
        let call_result = self
            .inner
            .subscribe(subscribe_method, params, unsubscribe_method)
            .await;
        self.inspect_call_result(origin, call_result)
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Notif: DeserializeOwned,
    {
        let origin = CallOrigin::Subscription(method);
        self.limit_rate(origin).await?;
        self.inspect_call_result(origin, self.inner.subscribe_to_method(method).await)
    }
}

/// Builder for the [`Client`].
pub struct ClientBuilder<Net, C = HttpClient> {
    client: C,
    url: SensitiveUrl,
    rate_limit: (usize, Duration),
    report_config: bool,
    network: Net,
}

impl<Net: fmt::Debug, C: 'static> fmt::Debug for ClientBuilder<Net, C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("L2ClientBuilder")
            .field("client", &any::type_name::<C>())
            .field("url", &self.url)
            .field("rate_limit", &self.rate_limit)
            .field("report_config", &self.report_config)
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

impl<Net: Network, C: ClientBase> ClientBuilder<Net, C> {
    /// Wraps the provided client.
    fn new(client: C, url: SensitiveUrl) -> Self {
        Self {
            client,
            url,
            rate_limit: (1, Duration::ZERO),
            report_config: true,
            network: Net::default(),
        }
    }

    /// Specifies the network to be used by the client. The network is logged and is used as a metrics label.
    pub fn for_network(mut self, network: Net) -> Self {
        self.network = network;
        self
    }

    /// Sets the rate limit for the client. The rate limit is applied across all client instances,
    /// including cloned ones.
    pub fn with_allowed_requests_per_second(mut self, rps: NonZeroUsize) -> Self {
        let rps = usize::from(rps);
        // Define the rate limiting window to be sufficiently small so that we don't get stampeding requests.
        self.rate_limit = match rps {
            1..=24 => (1, Duration::from_secs(1) / rps as u32),
            // Round requests per window up if necessary. The relative error of this rounding is selected to be <20%
            // in all cases.
            25..=49 => (rps.div_ceil(5), Duration::from_millis(200)),
            50..=99 => (rps.div_ceil(10), Duration::from_millis(100)),
            _ => (rps.div_ceil(20), Duration::from_millis(50)),
        };
        self
    }

    /// Allows switching off config reporting for this client in logs and metrics. This is useful if a client is a short-living one
    /// and is not injected as a dependency.
    pub fn report_config(mut self, report: bool) -> Self {
        self.report_config = report;
        self
    }

    /// Builds the client.
    pub fn build(self) -> Client<Net, C> {
        let rate_limit = SharedRateLimit::new(self.rate_limit.0, self.rate_limit.1);
        if self.report_config {
            tracing::info!(
                "Creating JSON-RPC client for network {:?} with inner client: {:?} and rate limit: {:?}",
                self.network,
                self.client,
                self.rate_limit
            );
            INFO_METRICS.observe_config(self.network.metric_label(), &rate_limit);
        }

        Client {
            inner: self.client,
            url: self.url,
            rate_limit,
            component_name: "",
            metrics: &METRICS[&ClientLabels {
                network: self.network.metric_label(),
                component: "",
            }],
            network: self.network,
        }
    }
}

#[derive(Debug, Default)]
#[must_use = "stats should be reported"]
struct AcquireStats {
    was_waiting: bool,
    total_sleep_time: Duration,
}

#[derive(Debug)]
enum SharedRateLimitState {
    Limited {
        until: Instant,
    },
    Ready {
        until: Instant,
        remaining_requests: usize,
    },
}

#[derive(Debug)]
struct SharedRateLimit {
    rate_limit: usize,
    rate_limit_window: Duration,
    state: Arc<Mutex<SharedRateLimitState>>,
}

impl Clone for SharedRateLimit {
    fn clone(&self) -> Self {
        Self {
            rate_limit: self.rate_limit,
            rate_limit_window: self.rate_limit_window,
            state: self.state.clone(),
        }
    }
}

impl SharedRateLimit {
    fn new(rate_limit: usize, rate_limit_window: Duration) -> Self {
        let state = Arc::new(Mutex::new(SharedRateLimitState::Ready {
            until: Instant::now(),
            remaining_requests: rate_limit,
        }));
        Self {
            rate_limit,
            rate_limit_window,
            state,
        }
    }

    /// Acquires the specified number of requests waiting if necessary. If the number of requests exceeds
    /// the capacity of the limiter, it is saturated (i.e., this method cannot hang indefinitely or panic
    /// in this case).
    ///
    /// This implementation is similar to [`RateLimit`] middleware in Tower, but is shared among client instances.
    ///
    /// [`RateLimit`]: https://docs.rs/tower/latest/tower/limit/struct.RateLimit.html
    async fn acquire(&self, request_count: usize) -> AcquireStats {
        let mut stats = AcquireStats::default();
        let mut state = loop {
            // A separate scope is required to not hold a mutex guard across the `await` point,
            // which is not only semantically incorrect, but also makes the future `!Send` (unfortunately,
            // async Rust doesn't seem to understand non-lexical lifetimes).
            let now = Instant::now();
            let until = {
                let mut state = self.state.lock().expect("state is poisoned");
                match &*state {
                    SharedRateLimitState::Ready { .. } => break state,
                    SharedRateLimitState::Limited { until } if *until <= now => {
                        // At this point, local time is `>= until`; thus, the state should be reset.
                        // Because we hold an exclusive lock on `state`, there's no risk of a data race.
                        *state = SharedRateLimitState::Ready {
                            until: now + self.rate_limit_window,
                            remaining_requests: self.rate_limit,
                        };
                        break state;
                    }
                    SharedRateLimitState::Limited { until } => *until,
                }
            };

            stats.was_waiting = true;
            stats.total_sleep_time += until.duration_since(now);
            tokio::time::sleep_until(until).await;
        };
        let SharedRateLimitState::Ready {
            until,
            remaining_requests,
        } = &mut *state
        else {
            unreachable!();
        };

        let now = Instant::now();
        // Reset the period if it has elapsed.
        if now >= *until {
            *until = now + self.rate_limit_window;
            *remaining_requests = self.rate_limit;
        }
        *remaining_requests = remaining_requests.saturating_sub(request_count);
        if *remaining_requests == 0 {
            *state = SharedRateLimitState::Limited { until: *until };
        }
        stats
    }
}
