/// Configuration for the essential observability stack, like
/// logging and sentry integration.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfig {
    /// URL of the Sentry instance to send events to.
    pub sentry_url: Option<String>,
    /// Name of the environment to use in Sentry.
    pub sentry_environment: Option<String>,
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub opentelemetry_level: Option<String>,
    /// Opentelemetry HTTP collector endpoint.
    pub otlp_endpoint: Option<String>,
    /// Format of the logs as expected by the `vlog` crate.
    /// Currently must be either `plain` or `json`.
    pub log_format: String,
}
