/// Configuration for the essential observability stack, like
/// logging and sentry integration.
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// URL of the Sentry instance to send events to.
    pub sentry_url: Option<String>,
    /// Name of the environment to use in Sentry.
    pub sentry_environment: Option<String>,
    /// Format of the logs as expected by the `vlog` crate.
    /// Currently must be either `plain` or `json`.
    pub log_format: String,
}
