use std::str::FromStr;

use opentelemetry::{
    sdk::{
        propagation::TraceContextPropagator,
        trace::{self, RandomIdGenerator, Sampler},
        Resource,
    },
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tracing_subscriber::{registry::LookupSpan, EnvFilter, Layer};

#[derive(Debug)]
pub struct OpenTelemetry {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub opentelemetry_level: OpenTelemetryLevel,
    /// Opentelemetry HTTP collector endpoint.
    pub otlp_endpoint: String,
    /// Service name to use
    pub service_name: Option<String>,
}

impl OpenTelemetry {
    pub fn new(
        opentelemetry_level: &str,
        otlp_endpoint: String,
    ) -> Result<Self, OpenTelemetryLevelError> {
        Ok(Self {
            opentelemetry_level: opentelemetry_level.parse()?,
            otlp_endpoint,
            service_name: None,
        })
    }

    pub(super) fn into_layer<S>(self) -> impl Layer<S>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        let filter = match self.opentelemetry_level {
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

        let mut resource = vec![];
        if let Some(service_name) = self.service_name {
            resource.push(KeyValue::new(SERVICE_NAME, service_name));
        }

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .http()
                    .with_endpoint(self.otlp_endpoint),
            )
            .with_trace_config(
                trace::config()
                    .with_sampler(Sampler::AlwaysOn)
                    .with_id_generator(RandomIdGenerator::default())
                    .with_resource(Resource::new(resource)),
            )
            .install_batch(::opentelemetry::runtime::Tokio)
            .unwrap();

        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(filter)
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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpenTelemetryLevelError {
    #[error("Invalid OpenTelemetry level format")]
    InvalidFormat,
}

impl FromStr for OpenTelemetryLevel {
    type Err = OpenTelemetryLevelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(OpenTelemetryLevel::OFF),
            "info" => Ok(OpenTelemetryLevel::INFO),
            "debug" => Ok(OpenTelemetryLevel::DEBUG),
            "trace" => Ok(OpenTelemetryLevel::TRACE),
            _ => Err(OpenTelemetryLevelError::InvalidFormat),
        }
    }
}
