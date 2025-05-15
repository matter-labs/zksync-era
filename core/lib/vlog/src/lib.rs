//! This crate contains the observability subsystem.
//! It is responsible for providing a centralized interface for consistent observability configuration.

use std::{fmt, sync::mpsc, thread, time::Duration};

use ::sentry::ClientInitGuard;
use anyhow::Context as _;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use crate::{logs::Logs, opentelemetry::OpenTelemetry, sentry::Sentry};

pub mod logs;
#[cfg(feature = "node_framework")]
pub mod node;
pub mod opentelemetry;
pub mod prometheus;
pub mod sentry;

/// Internal trait used in `ObservabilityGuard::with_timeout()` to inspect action results.
trait InspectResults {
    fn inspect_results(&self, action_name: &str);
}

impl<E: fmt::Debug> InspectResults for Result<(), E> {
    fn inspect_results(&self, action_name: &str) {
        if let Err(err) = self {
            tracing::warn!("Failed {action_name}: {err:?}");
        }
    }
}

impl<E: fmt::Debug> InspectResults for Vec<Result<(), E>> {
    fn inspect_results(&self, action_name: &str) {
        for partial_res in self {
            if let Err(err) = partial_res {
                tracing::warn!("Failed {action_name}: {err:?}");
            }
        }
    }
}

/// Builder for the observability subsystem.
/// Currently capable of configuring logging output and sentry integration.
#[derive(Debug, Default)]
pub struct ObservabilityBuilder {
    logs: Option<Logs>,
    opentelemetry_layer: Option<OpenTelemetry>,
    sentry: Option<Sentry>,
}

/// Guard for the observability subsystem.
/// Releases configured integrations upon being dropped.
pub struct ObservabilityGuard {
    /// Opentelemetry traces provider
    otlp_tracing_provider: Option<opentelemetry_sdk::trace::TracerProvider>,
    /// Opentelemetry logs provider
    otlp_logging_provider: Option<opentelemetry_sdk::logs::LoggerProvider>,
    /// Sentry client guard
    sentry_guard: Option<ClientInitGuard>,
}

impl ObservabilityGuard {
    /// Forces flushing of pending events. This method is blocking.
    fn force_flush(&mut self) {
        // We don't want to wait for too long.
        const FLUSH_TIMEOUT: Duration = Duration::from_secs(1);

        if let Some(sentry_guard) = &self.sentry_guard {
            sentry_guard.flush(Some(FLUSH_TIMEOUT));
            tracing::info!("Sentry events are flushed");
        }

        Self::with_timeout(
            FLUSH_TIMEOUT,
            "flushing tracing spans",
            &mut self.otlp_tracing_provider,
            opentelemetry_sdk::trace::TracerProvider::force_flush,
        );
        Self::with_timeout(
            FLUSH_TIMEOUT,
            "flushing tracing logs",
            &mut self.otlp_logging_provider,
            opentelemetry_sdk::logs::LoggerProvider::force_flush,
        );
    }

    /// Performs an action on the OpenTelemetry provider. If the action times out, the provider is lost, but the control flow proceeds
    /// (= a node doesn't hang up during shutdown).
    ///
    /// Motivated by OpenTelemetry providers hanging up indefinitely on flushing / shutdown if the configured telemetry URL is unreachable.
    fn with_timeout<T, R>(
        timeout: Duration,
        action_name: &'static str,
        maybe_provider: &mut Option<T>,
        action: fn(&T) -> R,
    ) where
        T: Send + 'static,
        R: InspectResults + Send + 'static,
    {
        if let Some(provider) = maybe_provider.take() {
            let (result_sender, result_receiver) = mpsc::sync_channel(1);
            let join_handle = thread::spawn(move || {
                result_sender.send((action(&provider), provider)).ok();
            });
            match result_receiver.recv_timeout(timeout) {
                Ok((results, provider)) => {
                    tracing::info!("Completed {action_name}");
                    *maybe_provider = Some(provider);
                    results.inspect_results(action_name);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if join_handle.join().is_err() {
                        tracing::error!("Panicked while {action_name}");
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!(?timeout, "Timed out {action_name}");
                }
            }
        } else {
            tracing::debug!("Skipped {action_name}: no provider present");
        }
    }

    /// Shutdown the observability subsystem.
    /// It will stop any background tasks and release resources.
    pub fn shutdown(&mut self) {
        // We don't want to wait for too long.
        const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

        // `take` here and below ensures that we don't have any access to the deinitialized resources.
        if let Some(sentry_guard) = self.sentry_guard.take() {
            sentry_guard.close(Some(SHUTDOWN_TIMEOUT));
            tracing::info!("Sentry client is shut down");
        }

        Self::with_timeout(
            SHUTDOWN_TIMEOUT,
            "shutting down OTLP tracing provider",
            &mut self.otlp_tracing_provider,
            opentelemetry_sdk::trace::TracerProvider::shutdown,
        );
        Self::with_timeout(
            SHUTDOWN_TIMEOUT,
            "shutting down OTLP logging provider",
            &mut self.otlp_logging_provider,
            opentelemetry_sdk::logs::LoggerProvider::shutdown,
        );
    }
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        self.force_flush();
        self.shutdown();
    }
}

impl std::fmt::Debug for ObservabilityGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservabilityGuard").finish()
    }
}

impl ObservabilityBuilder {
    /// Creates a new builder with default values.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_logs(mut self, logs: Option<Logs>) -> Self {
        self.logs = logs;
        self
    }

    pub fn with_opentelemetry(mut self, opentelemetry: Option<OpenTelemetry>) -> Self {
        self.opentelemetry_layer = opentelemetry;
        self
    }

    pub fn with_sentry(mut self, sentry: Option<Sentry>) -> Self {
        self.sentry = sentry;
        self
    }

    /// Tries to initialize the observability subsystem. Returns an error if it's already initialized.
    /// This is mostly useful in tests.
    pub fn try_build(self) -> anyhow::Result<ObservabilityGuard> {
        let logs = self.logs.unwrap_or_default();
        logs.install_panic_hook();

        // For now we use logs filter as a global filter for subscriber.
        // Later we may want to enforce each layer to have its own filter.
        let global_filter = logs.build_filter();

        let logs_layer = logs.into_layer();
        let (otlp_tracing_provider, otlp_tracing_layer) = self
            .opentelemetry_layer
            .as_ref()
            .and_then(|layer| layer.tracing_layer())
            .unzip();
        let (otlp_logging_provider, otlp_logging_layer) = self
            .opentelemetry_layer
            .and_then(|layer| layer.logs_layer())
            .unzip();

        tracing_subscriber::registry()
            .with(global_filter)
            .with(logs_layer)
            .with(otlp_tracing_layer)
            .with(otlp_logging_layer)
            .try_init()
            .context("failed installing global tracer / logger")?;

        let sentry_guard = self.sentry.map(|sentry| sentry.install());

        Ok(ObservabilityGuard {
            otlp_tracing_provider,
            otlp_logging_provider,
            sentry_guard,
        })
    }

    /// Initializes the observability subsystem.
    pub fn build(self) -> ObservabilityGuard {
        self.try_build().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;
    use crate::opentelemetry::OpenTelemetryLevel;

    #[tokio::test]
    async fn otlp_provider_does_not_hang_up_on_shutdown() {
        let bogus_url = "http://non-existing-address-leading-to-otlp-flushing-hanging-up"
            .parse::<Url>()
            .unwrap();
        let _guard = ObservabilityBuilder::new()
            .with_opentelemetry(Some(OpenTelemetry {
                opentelemetry_level: OpenTelemetryLevel::TRACE,
                tracing_endpoint: Some(bogus_url.clone()),
                logging_endpoint: Some(bogus_url),
                service: Default::default(),
            }))
            .try_build()
            .ok();
        tracing::info_span!("test").in_scope(|| {
            tracing::info!("This is a log");
        });
    }
}
