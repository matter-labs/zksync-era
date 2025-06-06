use std::{backtrace::Backtrace, str::FromStr};

use tracing_subscriber::{fmt, registry::LookupSpan, EnvFilter, Layer};

mod layer;

/// Specifies the format of the logs in stdout.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    #[default]
    Plain,
    Json,
}

impl FromStr for LogFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plain" => Ok(Self::Plain),
            "json" => Ok(Self::Json),
            _ => anyhow::bail!("unexpected logs format: {s:?}, expected one of 'plain', 'json'"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Logs {
    format: LogFormat,
    log_directives: Option<String>,
    disable_default_logs: bool,
}

impl Logs {
    pub fn new(format: LogFormat) -> Self {
        Self {
            format,
            log_directives: None,
            disable_default_logs: false,
        }
    }

    /// Builds a filter for the logs.
    ///
    /// Unless `disable_default_logs` was set, uses `zksync=info` as a default which is then merged
    /// with user-defined directives. Provided directives can extend/override the default value.
    ///
    /// The provided default convers all the crates with a name starting with `zksync` (per `tracing`
    /// [documentation][1]), which is a good enough default for any project.
    ///
    /// If `log_directives` are provided via `with_log_directives`, they will be used.
    /// Otherwise, the value will be parsed from the environment variable `RUST_LOG`.
    ///
    /// [1]: https://docs.rs/tracing-subscriber/0.3.18/tracing_subscriber/filter/targets/struct.Targets.html#filtering-with-targets
    pub(super) fn build_filter(&self) -> EnvFilter {
        let mut directives = if self.disable_default_logs {
            "".to_string()
        } else {
            "zksync=info,".to_string()
        };
        if let Some(log_directives) = &self.log_directives {
            directives.push_str(log_directives);
        } else if let Ok(env_directives) = std::env::var(EnvFilter::DEFAULT_ENV) {
            directives.push_str(&env_directives);
        };
        EnvFilter::new(directives)
    }

    pub fn with_log_directives(mut self, log_directives: Option<String>) -> Self {
        self.log_directives = log_directives;
        self
    }

    pub fn disable_default_logs(mut self) -> Self {
        self.disable_default_logs = true;
        self
    }

    pub fn install_panic_hook(&self) {
        // Check whether we need to change the default panic handler.
        // Note that this must happen before we initialize Sentry, since otherwise
        // Sentry's panic handler will also invoke the default one, resulting in unformatted
        // panic info being output to stderr.
        if matches!(self.format, LogFormat::Json) {
            // Remove any existing hook. We expect that no hook is set by default.
            let _ = std::panic::take_hook();
            // Override the default panic handler to print the panic in JSON format.
            std::panic::set_hook(Box::new(json_panic_handler));
        };
    }

    pub fn into_layer<S>(self) -> impl Layer<S>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        let filter = self.build_filter();
        let layer = match self.format {
            LogFormat::Plain => layer::LogsLayer::Plain(fmt::Layer::new()),
            LogFormat::Json => {
                let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
                let json_layer = fmt::Layer::default()
                    .with_file(true)
                    .with_line_number(true)
                    .with_timer(timer)
                    .json();
                layer::LogsLayer::Json(json_layer)
            }
        };
        layer.with_filter(filter)
    }
}

#[allow(deprecated)] // Not available yet on stable, so we can't switch right now.
fn json_panic_handler(panic_info: &std::panic::PanicInfo) {
    let backtrace = Backtrace::force_capture();
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
