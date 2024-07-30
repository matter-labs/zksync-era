//! This crate contains the observability subsystem.
//! It is responsible for providing a centralized interface for consistent observability configuration.

use std::time::Duration;

use ::sentry::ClientInitGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use crate::{logs::Logs, opentelemetry::OpenTelemetry, sentry::Sentry};

pub mod logs;
pub mod opentelemetry;
pub mod prometheus;
pub mod sentry;

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
    /// Opentelemetry provider. Can be used to force flush spans.
    otlp_provider: Option<opentelemetry_sdk::trace::TracerProvider>,
    sentry_guard: Option<ClientInitGuard>,
}

impl ObservabilityGuard {
    /// Forces flushing of pending events.
    /// This method is blocking.
    pub fn force_flush(&self) {
        // We don't want to wait for too long.
        const FLUSH_TIMEOUT: Duration = Duration::from_secs(1);

        if let Some(sentry_guard) = &self.sentry_guard {
            sentry_guard.flush(Some(FLUSH_TIMEOUT));
        }

        if let Some(provider) = &self.otlp_provider {
            for result in provider.force_flush() {
                if let Err(err) = result {
                    tracing::warn!("Flushing the spans failed: {err:?}");
                }
            }
        }
    }

    /// Shutdown the observability subsystem.
    /// It will stop the background tasks like collec
    pub fn shutdown(&self) {
        // We don't want to wait for too long.
        const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

        if let Some(sentry_guard) = &self.sentry_guard {
            sentry_guard.close(Some(SHUTDOWN_TIMEOUT));
        }
        if let Some(provider) = &self.otlp_provider {
            if let Err(err) = provider.shutdown() {
                tracing::warn!("Shutting down the provider failed: {err:?}");
            }
        }
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

    /// Initializes the observability subsystem.
    pub fn build(self) -> ObservabilityGuard {
        let logs = self.logs.unwrap_or_default();
        logs.install_panic_hook();

        // For now we use logs filter as a global filter for subscriber.
        // Later we may want to enforce each layer to have its own filter.
        let global_filter = logs.build_filter();

        let logs_layer = logs.into_layer();
        let (otlp_provider, otlp_layer) = self
            .opentelemetry_layer
            .map(|layer| layer.into_layer())
            .unzip();

        tracing_subscriber::registry()
            .with(global_filter)
            .with(logs_layer)
            .with(otlp_layer)
            .init();

        let sentry_guard = self.sentry.map(|sentry| sentry.install());

        ObservabilityGuard {
            otlp_provider,
            sentry_guard,
        }
    }
}
