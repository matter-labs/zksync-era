use std::str::FromStr;

use opentelemetry::{trace::TracerProvider, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    trace::{RandomIdGenerator, Sampler},
    Resource,
};
use opentelemetry_semantic_conventions::resource::{
    K8S_NAMESPACE_NAME, K8S_POD_NAME, SERVICE_NAME,
};
use tracing_subscriber::{registry::LookupSpan, EnvFilter, Layer};
use url::Url;

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
        let mut attributes = vec![];
        if let Some(pod_name) = self.k8s_pod_name {
            attributes.push(KeyValue::new(K8S_POD_NAME, pod_name));
        }
        if let Some(pod_namespace) = self.k8s_namespace_name {
            attributes.push(KeyValue::new(K8S_NAMESPACE_NAME, pod_namespace));
        }
        if let Some(service_name) = self.service_name {
            attributes.push(KeyValue::new(SERVICE_NAME, service_name));
        }
        Resource::new(attributes)
    }
}

#[derive(Debug)]
pub struct OpenTelemetry {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub opentelemetry_level: OpenTelemetryLevel,
    /// Opentelemetry HTTP collector endpoint.
    pub otlp_endpoint: Url,
    /// Information about service
    pub service: Option<ServiceDescriptor>,
}

impl OpenTelemetry {
    pub fn new(
        opentelemetry_level: &str,
        otlp_endpoint: String,
    ) -> Result<Self, OpenTelemetryLayerError> {
        Ok(Self {
            opentelemetry_level: opentelemetry_level.parse()?,
            otlp_endpoint: otlp_endpoint
                .parse()
                .map_err(|e| OpenTelemetryLayerError::InvalidUrl(otlp_endpoint, e))?,
            service: None,
        })
    }

    pub fn with_service_descriptor(mut self, service: ServiceDescriptor) -> Self {
        self.service = Some(service);
        self
    }

    pub(super) fn into_layer<S>(self) -> (opentelemetry_sdk::trace::TracerProvider, impl Layer<S>)
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

        let service = self.service.unwrap_or_default().fill_from_env();
        let service_name = service
            .service_name
            .clone()
            .unwrap_or_else(|| "zksync_vlog".to_string());
        let resource = service.into_otlp_resource();

        let exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(self.otlp_endpoint)
            .build_span_exporter()
            .expect("Failed to create OTLP exporter"); // URL is validated.

        let config = opentelemetry_sdk::trace::Config::default()
            .with_id_generator(RandomIdGenerator::default())
            .with_sampler(Sampler::AlwaysOn)
            .with_resource(resource);

        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_config(config)
            .build();

        // TODO: Version and other metadata
        let tracer = provider.tracer_builder(service_name).build();

        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(filter);

        (provider, layer)
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
pub enum OpenTelemetryLayerError {
    #[error("Invalid OpenTelemetry level format")]
    InvalidFormat,
    #[error("Invalid URL: \"{0}\" - {1}")]
    InvalidUrl(String, url::ParseError),
}

impl FromStr for OpenTelemetryLevel {
    type Err = OpenTelemetryLayerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(OpenTelemetryLevel::OFF),
            "info" => Ok(OpenTelemetryLevel::INFO),
            "debug" => Ok(OpenTelemetryLevel::DEBUG),
            "trace" => Ok(OpenTelemetryLevel::TRACE),
            _ => Err(OpenTelemetryLayerError::InvalidFormat),
        }
    }
}
