//! A set of logging macros that print not only timestamp and log level,
//! but also filename, line and column.
//!
//! They behave just like usual tracing::warn, tracing::info, etc.
//! For warn and error macros we are adding file line and column to tracing variables
//!
//! The format of the logs in stdout can be `plain` or` json` and is set by the `MISC_LOG_FORMAT` env variable.
//!
//! Full documentation for the `tracing` crate here https://docs.rs/tracing/
//!
//! Integration with sentry for catching errors and react on them immediately
//! https://docs.sentry.io/platforms/rust/
//!

use std::{borrow::Cow, str::FromStr};

use opentelemetry::sdk::{resource::Resource, trace::Sampler};
use opentelemetry::trace::{TraceContextExt, TraceId};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use sentry::protocol::Event;
use sentry::{types::Dsn, ClientInitGuard, ClientOptions};
use std::backtrace::Backtrace;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub use chrono as __chrono;
pub use sentry as __sentry;
pub use tracing as __tracing;
pub use tracing::{debug, info, log, trace};

fn get_trace_id() -> TraceId {
    let span = tracing::span::Span::current();
    span.context().span().span_context().trace_id()
}

#[macro_export]
macro_rules! warn {
    ($fmt:expr) => {{
        $crate::__tracing::warn!(
            file=file!(),
            line=line!(),
            column=column!(),
            $fmt,
        );
        // $crate::__sentry::capture_event($crate::__sentry::protocol::Event {
        //     fingerprint: ::std::borrow::Cow::Borrowed(&[::std::borrow::Cow::Borrowed($fmt)]),
        //     message: Some(format!($fmt)),
        //     level: $crate::__sentry::Level::Warning,
        //     ..Default::default()
        // });
    }};

    ($fmt:expr, $($args:tt)*) => {
        {
            $crate::__tracing::warn!(
                file=file!(),
                line=line!(),
                column=column!(),
                $fmt,
                $($args)*
            );
            // $crate::__sentry::capture_event($crate::__sentry::protocol::Event {
            //     fingerprint: ::std::borrow::Cow::Borrowed(&[::std::borrow::Cow::Borrowed($fmt)]),
            //     message: Some(format!($fmt, $($args)*)),
            //     level: $crate::__sentry::Level::Warning,
            //     ..Default::default()
            // });
        }
    };
}

#[macro_export]
macro_rules! panic {
    ($fmt:expr) => {{
        $crate::__tracing::error!(
            file=file!(),
            line=line!(),
            column=column!(),
            $fmt,
        );
        $crate::__sentry::capture_event($crate::__sentry::protocol::Event {
            fingerprint: ::std::borrow::Cow::Borrowed(&[::std::borrow::Cow::Borrowed($fmt)]),
            message: Some(format!($fmt)),
            level: $crate::__sentry::Level::Fatal,
            ..Default::default()
        });
    }};
    ($fmt:expr, $($args:tt)*) => {
        {
            $crate::__tracing::error!(
                file=file!(),
                line=line!(),
                column=column!(),
                $fmt,
                $($args)*
            );
            $crate::__sentry::capture_event($crate::__sentry::protocol::Event {
                fingerprint: ::std::borrow::Cow::Borrowed(&[::std::borrow::Cow::Borrowed($fmt)]),
                message: Some(format!($fmt, $($args)*)),
                level: $crate::__sentry::Level::Fatal,
                ..Default::default()
            });
        }
    };
}

#[macro_export]
macro_rules! error {
    ($fmt:expr) => {{
        $crate::__tracing::error!(
            file=file!(),
            line=line!(),
            column=column!(),
            $fmt,
        );
        $crate::__sentry::capture_event($crate::__sentry::protocol::Event {
            fingerprint: ::std::borrow::Cow::Borrowed(&[::std::borrow::Cow::Borrowed($fmt)]),
            message: Some(format!($fmt)),
            level: $crate::__sentry::Level::Error,
            ..Default::default()
        });
    }};
    ($fmt:expr, $($args:tt)*) => {
        {
            $crate::__tracing::error!(
                file=file!(),
                line=line!(),
                column=column!(),
                $fmt,
                $($args)*
            );
            $crate::__sentry::capture_event($crate::__sentry::protocol::Event {
                fingerprint: ::std::borrow::Cow::Borrowed(&[::std::borrow::Cow::Borrowed($fmt)]),
                message: Some(format!($fmt, $($args)*)),
                level: $crate::__sentry::Level::Error,
                ..Default::default()
            });
        }
    };
}

fn get_sentry_url() -> Option<Dsn> {
    if let Ok(sentry_url) = std::env::var("MISC_SENTRY_URL") {
        if let Ok(sentry_url) = Dsn::from_str(sentry_url.as_str()) {
            return Some(sentry_url);
        }
    }
    None
}

fn get_otlp_url() -> Option<String> {
    std::env::var("MISC_OTLP_URL").ok().and_then(|url| {
        if url.to_lowercase() == "unset" {
            None
        } else {
            Some(url)
        }
    })
}

pub const DEFAULT_SAMPLING_RATIO: f64 = 0.1;

fn get_sampling_ratio() -> f64 {
    std::env::var("MISC_SAMPLING_RATIO")
        .map(|x| x.as_str().parse::<f64>().unwrap())
        .unwrap_or(DEFAULT_SAMPLING_RATIO)
}

/// Initialize logging with tracing and set up log format
pub fn init() {
    let log_format = std::env::var("MISC_LOG_FORMAT").unwrap_or_else(|_| "plain".to_string());
    let service_name =
        std::env::var("SERVICE_NAME").unwrap_or_else(|_| "UNKNOWN_SERVICE".to_string());
    let namespace_name =
        std::env::var("POD_NAMESPACE").unwrap_or_else(|_| "UNKNOWN_NAMESPACE".to_string());
    let pod_name = std::env::var("POD_NAME").unwrap_or_else(|_| "UNKNOWN_POD".to_string());
    let opentelemetry = get_otlp_url().map(|url| {
        let otlp_exporter = opentelemetry_otlp::new_exporter().http().with_endpoint(url);
        let sampler = Sampler::TraceIdRatioBased(get_sampling_ratio());

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(otlp_exporter)
            .with_trace_config(
                opentelemetry::sdk::trace::config()
                    .with_resource(Resource::new(vec![
                        KeyValue::new(
                            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                            service_name,
                        ),
                        KeyValue::new(
                            opentelemetry_semantic_conventions::resource::K8S_NAMESPACE_NAME,
                            namespace_name,
                        ),
                        KeyValue::new(
                            opentelemetry_semantic_conventions::resource::K8S_POD_NAME,
                            pod_name,
                        ),
                    ]))
                    .with_sampler(sampler),
            )
            .install_simple()
            .unwrap();
        tracing_opentelemetry::layer().with_tracer(tracer)
    });
    match log_format.as_str() {
        "plain" => {
            if let Some(opentelemetry) = opentelemetry {
                tracing_subscriber::registry()
                    .with(opentelemetry)
                    .with(fmt::Layer::default())
                    .with(tracing_subscriber::EnvFilter::from_default_env())
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(fmt::Layer::default())
                    .with(tracing_subscriber::EnvFilter::from_default_env())
                    .init();
            }
        }
        "json" => {
            let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
            // must be set before sentry hook for sentry to function
            install_pretty_panic_hook();
            if let Some(opentelemetry) = opentelemetry {
                tracing_subscriber::registry()
                    .with(opentelemetry)
                    .with(fmt::Layer::default().with_timer(timer).json())
                    .with(tracing_subscriber::EnvFilter::from_default_env())
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(fmt::Layer::default().with_timer(timer).json())
                    .with(tracing_subscriber::EnvFilter::from_default_env())
                    .init();
            }
        }
        _ => panic!("MISC_LOG_FORMAT has an unexpected value {}", log_format),
    };
}

/// If the sentry URL is provided via an environment variable, this function will initialize sentry.
/// Returns a sentry client guard. The full description can be found in the official documentation:
/// https://docs.sentry.io/platforms/rust/#configure
pub fn init_sentry() -> Option<ClientInitGuard> {
    get_sentry_url().map(|sentry_url| {
        // Either use the environment provided for EN, or load it from default main node config.
        let environment = match std::env::var("EN_SENTRY_ENVIRONMENT") {
            Ok(environment) => environment,
            Err(_) => {
                // No EN environment provided, load it from the main node config.
                let l1_network = std::env::var("CHAIN_ETH_NETWORK").expect("Must be set");
                let l2_network = std::env::var("CHAIN_ETH_ZKSYNC_NETWORK").expect("Must be set");

                format!("{} - {}", l1_network, l2_network)
            }
        };

        let options = sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: Some(Cow::from(environment)),
            attach_stacktrace: true,
            ..Default::default()
        }
        .add_integration(TraceIdToSentry);

        sentry::init((sentry_url, options))
    })
}

/// Format panics like vlog::error
fn install_pretty_panic_hook() {
    // This hook does not use the previous one set because it leads to 2 logs:
    // the first is the default panic log and the second is from this code. To avoid this situation,
    // hook must be installed first
    std::panic::set_hook(Box::new(move |panic_info| {
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

        let backtrace_str = format!("{}", backtrace);
        let timestamp_str = format!("{}", timestamp.format("%Y-%m-%dT%H:%M:%S%.fZ"));

        let trace_id = get_trace_id();
        println!(
            "{}",
            serde_json::json!({
                "timestamp": timestamp_str,
                "trace_id": trace_id.to_string(),
                "level": "CRITICAL",
                "fields": {
                    "message": panic_message,
                    "location": panic_location,
                    "backtrace": backtrace_str,
                }
            })
        );
    }));
}

struct TraceIdToSentry;

impl sentry::Integration for TraceIdToSentry {
    fn process_event(
        &self,
        mut event: Event<'static>,
        _options: &ClientOptions,
    ) -> Option<Event<'static>> {
        let trace_id = get_trace_id();
        event
            .extra
            .insert("trace_id".to_string(), trace_id.to_string().into());
        Some(event)
    }
}
