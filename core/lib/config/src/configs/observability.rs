/// Configuration for the essential observability stack, like
/// logging and sentry integration.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfig {
    /// URL of the Sentry instance to send events to.
    pub sentry_url: Option<String>,
    /// Name of the environment to use in Sentry.
    pub sentry_environment: Option<String>,
    /// Opentelemetry configuration.
    pub opentelemetry: Option<OpentelemetryConfig>,
    /// Format of the logs as expected by the `vlog` crate.
    /// Currently must be either `plain` or `json`.
    pub log_format: String,
    /// Log directives in format that is used in `RUST_LOG`
    pub log_directives: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpentelemetryConfig {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub level: String,
    /// Opentelemetry HTTP traces collector endpoint.
    pub endpoint: String,
    /// Opentelemetry HTTP logs collector endpoing.
    /// This is optional, since right now the primary way to collect logs is via stdout.
    ///
    /// Important: sending logs via OTLP has only been tested locally, and the performance may be
    /// suboptimal in production environments.
    pub logs_endpoint: Option<String>,
}
