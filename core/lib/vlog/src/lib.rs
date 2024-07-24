//! This crate contains the observability subsystem.
//! It is responsible for providing a centralized interface for consistent observability configuration.

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
    _sentry_guard: Option<ClientInitGuard>,
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

        tracing_subscriber::registry()
            .with(logs.into_layer())
            .with(self.opentelemetry_layer.map(|layer| layer.into_layer()))
            .init();

        let sentry_guard = self.sentry.map(|sentry| sentry.install());

        ObservabilityGuard {
            _sentry_guard: sentry_guard,
        }
    }
}
