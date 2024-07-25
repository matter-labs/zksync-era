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
use opentelemetry_semantic_conventions::resource::{
    K8S_NAMESPACE_NAME, K8S_POD_NAME, SERVICE_NAME,
};
use tracing_subscriber::{registry::LookupSpan, EnvFilter, Layer};

/// Information about the service.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ServiceDescriptor {
    /// Name of the k8s pod.
    pub k8s_pod_name: Option<String>,
    /// Name of the k8s namespace.
    pub k8s_namespace_name: Option<String>,
    /// Name of the service.
    pub service_name: Option<String>,
}

impl ServiceDescriptor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_k8s_pod_name(mut self, k8s_pod_name: Option<String>) -> Self {
        self.k8s_pod_name = k8s_pod_name;
        self
    }

    pub fn with_k8s_namespace_name(mut self, k8s_namespace_name: Option<String>) -> Self {
        self.k8s_namespace_name = k8s_namespace_name;
        self
    }

    pub fn with_service_name(mut self, service_name: Option<String>) -> Self {
        self.service_name = service_name;
        self
    }

    /// Tries to fill empty fields from environment variables.
    ///
    /// The following environment variables are used:
    /// - `POD_NAME`
    /// - `POD_NAMESPACE`
    /// - `SERVICE_NAME`
    pub fn fill_from_env(mut self) -> Self {
        if self.k8s_pod_name.is_none() {
            self.k8s_pod_name = std::env::var("POD_NAME").ok();
        }
        if self.k8s_namespace_name.is_none() {
            self.k8s_namespace_name = std::env::var("POD_NAMESPACE").ok();
        }
        if self.service_name.is_none() {
            self.service_name = std::env::var("SERVICE_NAME").ok();
        }
        self
    }

    fn into_otlp_resource(self) -> Resource {
        let mut resource = vec![];
        if let Some(pod_name) = self.k8s_pod_name {
            resource.push(KeyValue::new(K8S_POD_NAME, pod_name));
        }
        if let Some(pod_namespace) = self.k8s_namespace_name {
            resource.push(KeyValue::new(K8S_NAMESPACE_NAME, pod_namespace));
        }
        if let Some(service_name) = self.service_name {
            resource.push(KeyValue::new(SERVICE_NAME, service_name));
        }
        Resource::new(resource)
    }
}

#[derive(Debug)]
pub struct OpenTelemetry {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub opentelemetry_level: OpenTelemetryLevel,
    /// Opentelemetry HTTP collector endpoint.
    pub otlp_endpoint: String,
    /// Information about service
    pub service: Option<ServiceDescriptor>,
}

impl OpenTelemetry {
    pub fn new(
        opentelemetry_level: &str,
        otlp_endpoint: String,
    ) -> Result<Self, OpenTelemetryLevelError> {
        Ok(Self {
            opentelemetry_level: opentelemetry_level.parse()?,
            otlp_endpoint,
            service: None,
        })
    }

    pub fn with_service_descriptor(mut self, service: ServiceDescriptor) -> Self {
        self.service = Some(service);
        self
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

        let resource = self
            .service
            .unwrap_or_default()
            .fill_from_env()
            .into_otlp_resource();

        // We can't know if we will be running within tokio context, so we will spawn
        // a separate thread for the exporter.
        let runtime = opentelemetry::runtime::TokioCurrentThread;

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
                    .with_resource(resource),
            )
            .install_batch(runtime)
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
