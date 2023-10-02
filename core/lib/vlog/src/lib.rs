//! This module contains the observability subsystem.
//! It is responsible for providing a centralized interface for consistent observability configuration.

use std::backtrace::Backtrace;
use std::borrow::Cow;
use std::panic::PanicInfo;

use sentry::{types::Dsn, ClientInitGuard};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Temporary re-export of `sentry::capture_message` aiming to simplify the transition from `vlog` to using
/// crates directly.
pub use sentry::capture_message;
pub use sentry::Level as AlertLevel;

/// Specifies the format of the logs in stdout.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    #[default]
    Plain,
    Json,
}

/// Builder for the observability subsystem.
/// Currently capable of configuring logging output and sentry integration.
#[derive(Debug, Default)]
pub struct ObservabilityBuilder {
    log_format: LogFormat,
    sentry_url: Option<Dsn>,
    sentry_environment: Option<String>,
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

    /// Sets the log format.
    /// Default is `LogFormat::Plain`.
    pub fn with_log_format(mut self, log_format: LogFormat) -> Self {
        self.log_format = log_format;
        self
    }

    /// Enables Sentry integration.
    /// Returns an error if the provided Sentry URL is invalid.
    pub fn with_sentry_url(
        mut self,
        sentry_url: &str,
    ) -> Result<Self, sentry::types::ParseDsnError> {
        let sentry_url = sentry_url.parse()?;
        self.sentry_url = Some(sentry_url);
        Ok(self)
    }

    /// Sets the Sentry environment ID.
    /// If not set, no environment will be provided in Sentry events.
    pub fn with_sentry_environment(mut self, environment: Option<String>) -> Self {
        self.sentry_environment = environment;
        self
    }

    /// Initializes the observability subsystem.
    pub fn build(self) -> ObservabilityGuard {
        // Initialize logs.
        match self.log_format {
            LogFormat::Plain => {
                tracing_subscriber::registry()
                    .with(tracing_subscriber::EnvFilter::from_default_env())
                    .with(fmt::Layer::default())
                    .init();
            }
            LogFormat::Json => {
                let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
                tracing_subscriber::registry()
                    .with(tracing_subscriber::EnvFilter::from_default_env())
                    .with(
                        fmt::Layer::default()
                            .with_file(true)
                            .with_line_number(true)
                            .with_timer(timer)
                            .json(),
                    )
                    .init();
            }
        };

        // Check whether we need to change the default panic handler.
        // Note that this must happen before we initialize Sentry, since otherwise
        // Sentry's panic handler will also invoke the default one, resulting in unformatted
        // panic info being output to stderr.
        if matches!(self.log_format, LogFormat::Json) {
            // Remove any existing hook. We expect that no hook is set by default.
            let _ = std::panic::take_hook();
            // Override the default panic handler to print the panic in JSON format.
            std::panic::set_hook(Box::new(json_panic_handler));
        };

        // Initialize the Sentry.
        let sentry_guard = if let Some(sentry_url) = self.sentry_url {
            let options = sentry::ClientOptions {
                release: sentry::release_name!(),
                environment: self.sentry_environment.map(Cow::from),
                attach_stacktrace: true,
                ..Default::default()
            };

            Some(sentry::init((sentry_url, options)))
        } else {
            None
        };

        ObservabilityGuard {
            _sentry_guard: sentry_guard,
        }
    }
}

/// Loads the log format from the environment variable according to the existing zkSync configuration scheme.
/// If the variable is not set, the default value is used.
///
/// This is a deprecated function existing for compatibility with the old configuration scheme.
/// Not recommended for use in new applications.
///
/// # Panics
///
/// Panics if the value of the variable is set, but is not `plain` or `json`.
#[deprecated(
    note = "This function will be removed in the future. Applications are expected to handle their configuration themselves."
)]
pub fn log_format_from_env() -> LogFormat {
    match std::env::var("MISC_LOG_FORMAT") {
        Ok(log_format) => match log_format.as_str() {
            "plain" => LogFormat::Plain,
            "json" => LogFormat::Json,
            _ => panic!("MISC_LOG_FORMAT has an unexpected value {}", log_format),
        },
        Err(_) => LogFormat::Plain,
    }
}

/// Loads the Sentry URL from the environment variable according to the existing zkSync configuration scheme.
/// If the environemnt value is present but the value is `unset`, `None` will be returned for compatibility with the
/// existing configuration setup.
///
/// This is a deprecated function existing for compatibility with the old configuration scheme.
/// Not recommended for use in new applications.
#[deprecated(
    note = "This function will be removed in the future. Applications are expected to handle their configuration themselves."
)]
pub fn sentry_url_from_env() -> Option<String> {
    match std::env::var("MISC_SENTRY_URL") {
        Ok(str) if str == "unset" => {
            // This bogus value may be provided an sentry is expected to just not be initialized in this case.
            None
        }
        Ok(str) => Some(str),
        Err(_) => None,
    }
}

/// Prepared the Sentry environment ID from the environment variable according to the existing zkSync configuration
/// scheme.
/// This function mimics like `vlog` configuration worked historically, e.g. it would also try to load environment
/// for the external node, and the EN variable is preferred if it is set.
///
/// This is a deprecated function existing for compatibility with the old configuration scheme.
/// Not recommended for use in new applications.
#[deprecated(
    note = "This function will be removed in the future. Applications are expected to handle their configuration themselves."
)]
pub fn environment_from_env() -> Option<String> {
    if let Ok(en_env) = std::env::var("EN_SENTRY_ENVIRONMENT") {
        return Some(en_env);
    }

    let l1_network = std::env::var("CHAIN_ETH_NETWORK").ok()?;
    let l2_network = std::env::var("CHAIN_ETH_ZKSYNC_NETWORK").ok()?;

    Some(format!("{} - {}", l1_network, l2_network))
}

fn json_panic_handler(panic_info: &PanicInfo) {
    let backtrace = Backtrace::capture();
    let timestamp = chrono::Utc::now();
    let panic_message = if let Some(s) = panic_info.payload().downcast_ref::<String>() {
        s.as_str()
    } else if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
        s
    } else {
        "Panic occurred without additional info"
    };

    let panic_location = panic_info
        .location()
        .map(|val| val.to_string())
        .unwrap_or_else(|| "Unknown location".to_owned());

    let backtrace_str = backtrace.to_string();
    let timestamp_str = timestamp.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string();

    println!(
        "{}",
        serde_json::json!({
            "timestamp": timestamp_str,
            "level": "CRITICAL",
            "fields": {
                "message": panic_message,
                "location": panic_location,
                "backtrace": backtrace_str,
            }
        })
    );
}
