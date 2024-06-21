//! This module contains the observability subsystem.
//! It is responsible for providing a centralized interface for consistent observability configuration.

use std::{backtrace::Backtrace, borrow::Cow, panic::PanicInfo, str::FromStr};

// Temporary re-export of `sentry::capture_message` aiming to simplify the transition from `vlog` to using
// crates directly.
use opentelemetry::{
    sdk::{
        propagation::TraceContextPropagator,
        trace::{self, RandomIdGenerator, Sampler, Tracer},
        Resource,
    },
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
pub use sentry::{capture_message, Level as AlertLevel};
use sentry::{types::Dsn, ClientInitGuard};
use serde::{de::Error, Deserialize, Deserializer};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    filter::Filtered,
    fmt,
    layer::{Layered, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

type TracingLayer<Inner> =
    Layered<Filtered<OpenTelemetryLayer<Inner, Tracer>, EnvFilter, Inner>, Inner>;

/// Specifies the format of the logs in stdout.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    #[default]
    Plain,
    Json,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plain => f.write_str("plain"),
            Self::Json => f.write_str("json"),
        }
    }
}

#[derive(Debug)]
pub struct LogFormatError(&'static str);

impl std::fmt::Display for LogFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for LogFormatError {}

impl FromStr for LogFormat {
    type Err = LogFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plain" => Ok(LogFormat::Plain),
            "json" => Ok(LogFormat::Json),
            _ => Err(LogFormatError("invalid log format")),
        }
    }
}

impl<'de> Deserialize<'de> for LogFormat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<Self>().map_err(D::Error::custom)
    }
}

// Doesn't define WARN and ERROR, because the highest verbosity of spans is INFO.
#[derive(Copy, Clone, Debug, Default)]
pub enum OpenTelemetryLevel {
    #[default]
    OFF,
    INFO,
    DEBUG,
    TRACE,
}

#[derive(Debug)]
pub struct OpenTelemetryLevelFormatError;

impl std::fmt::Display for OpenTelemetryLevelFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid OpenTelemetry level format")
    }
}

impl std::error::Error for OpenTelemetryLevelFormatError {}

impl FromStr for OpenTelemetryLevel {
    type Err = OpenTelemetryLevelFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(OpenTelemetryLevel::OFF),
            "info" => Ok(OpenTelemetryLevel::INFO),
            "debug" => Ok(OpenTelemetryLevel::DEBUG),
            "trace" => Ok(OpenTelemetryLevel::TRACE),
            _ => Err(OpenTelemetryLevelFormatError),
        }
    }
}

impl std::fmt::Display for OpenTelemetryLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            OpenTelemetryLevel::OFF => "off",
            OpenTelemetryLevel::INFO => "info",
            OpenTelemetryLevel::DEBUG => "debug",
            OpenTelemetryLevel::TRACE => "trace",
        };
        write!(f, "{}", str)
    }
}

#[derive(Clone, Debug)]
pub struct OpenTelemetryOptions {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub opentelemetry_level: OpenTelemetryLevel,
    /// Opentelemetry HTTP collector endpoint.
    pub otlp_endpoint: String,
    /// Logical service name to be used for exported events. See [`SERVICE_NAME`].
    pub service_name: String,
}

/// Builder for the observability subsystem.
/// Currently capable of configuring logging output and sentry integration.
#[derive(Debug, Default)]
pub struct ObservabilityBuilder {
    log_format: LogFormat,
    log_directives: Option<String>,
    sentry_url: Option<Dsn>,
    sentry_environment: Option<String>,
    opentelemetry_options: Option<OpenTelemetryOptions>,
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

    pub fn with_log_directives(mut self, log_level: String) -> Self {
        self.log_directives = Some(log_level);
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

    pub fn with_opentelemetry(
        mut self,
        opentelemetry_level: &str,
        otlp_endpoint: String,
        service_name: String,
    ) -> Result<Self, OpenTelemetryLevelFormatError> {
        self.opentelemetry_options = Some(OpenTelemetryOptions {
            opentelemetry_level: opentelemetry_level.parse()?,
            otlp_endpoint,
            service_name,
        });
        Ok(self)
    }

    fn add_opentelemetry_layer<S>(
        opentelemetry_level: OpenTelemetryLevel,
        otlp_endpoint: String,
        service_name: String,
        subscriber: S,
    ) -> TracingLayer<S>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        let filter = match opentelemetry_level {
            OpenTelemetryLevel::OFF => EnvFilter::new("off"),
            OpenTelemetryLevel::INFO => EnvFilter::new("info"),
            OpenTelemetryLevel::DEBUG => EnvFilter::new("debug"),
            OpenTelemetryLevel::TRACE => EnvFilter::new("trace"),
        };
        // `otel::tracing` should be a level info to emit opentelemetry trace & span
        // `otel` set to debug to log detected resources, configuration read and inferred
        let filter = filter
            .add_directive("otel::tracing=trace".parse().unwrap())
            .add_directive("otel=debug".parse().unwrap());

        let resource = vec![KeyValue::new(SERVICE_NAME, service_name)];

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .http()
                    .with_endpoint(otlp_endpoint),
            )
            .with_trace_config(
                trace::config()
                    .with_sampler(Sampler::AlwaysOn)
                    .with_id_generator(RandomIdGenerator::default())
                    .with_resource(Resource::new(resource)),
            )
            .install_batch(opentelemetry::runtime::Tokio)
            .unwrap();

        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(filter);
        subscriber.with(layer)
    }

    /// Builds a filter for the logs.
    ///
    /// Uses `zksync=info` as a default which is then merged with user-defined directives.
    /// Provided directives can extend/override the default value.
    ///
    /// The provided default convers all the crates with a name starting with `zksync` (per `tracing`
    /// [documentation][1]), which is a good enough default for any project.
    ///
    /// If `log_directives` are provided via `with_log_directives`, they will be used.
    /// Otherwise, the value will be parsed from the environment variable `RUST_LOG`.
    ///
    /// [1]: https://docs.rs/tracing-subscriber/0.3.18/tracing_subscriber/filter/targets/struct.Targets.html#filtering-with-targets
    fn build_filter(&self) -> EnvFilter {
        let mut directives = "zksync=info,".to_string();
        if let Some(log_directives) = &self.log_directives {
            directives.push_str(log_directives);
        } else if let Ok(env_directives) = std::env::var(EnvFilter::DEFAULT_ENV) {
            directives.push_str(&env_directives);
        };
        EnvFilter::new(directives)
    }

    /// Initializes the observability subsystem.
    pub fn build(self) -> ObservabilityGuard {
        // Initialize logs.
        let env_filter = self.build_filter();

        match self.log_format {
            LogFormat::Plain => {
                let subscriber = tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::Layer::default());
                if let Some(opts) = self.opentelemetry_options {
                    let subscriber = Self::add_opentelemetry_layer(
                        opts.opentelemetry_level,
                        opts.otlp_endpoint,
                        opts.service_name,
                        subscriber,
                    );
                    subscriber.init()
                } else {
                    subscriber.init()
                }
            }
            LogFormat::Json => {
                let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
                let subscriber = tracing_subscriber::registry().with(env_filter).with(
                    fmt::Layer::default()
                        .with_file(true)
                        .with_line_number(true)
                        .with_timer(timer)
                        .json(),
                );
                if let Some(opts) = self.opentelemetry_options {
                    let subscriber = Self::add_opentelemetry_layer(
                        opts.opentelemetry_level,
                        opts.otlp_endpoint,
                        opts.service_name,
                        subscriber,
                    );
                    subscriber.init()
                } else {
                    subscriber.init()
                }
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

fn json_panic_handler(panic_info: &PanicInfo) {
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
