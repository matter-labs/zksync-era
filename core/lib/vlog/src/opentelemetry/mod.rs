use std::str::FromStr;

use opentelemetry::{trace::TracerProvider, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::Sampler, Resource};
use opentelemetry_semantic_conventions::attribute;
use tracing_subscriber::{registry::LookupSpan, EnvFilter, Layer};
use url::Url;

/// Information about the service.
///
/// This information is initially filled as follows:
/// - Fields will be attempted to fetch from environment variables.
/// - If not found, a default values will be chosen.
///
/// For environment variable names and default values, see the constants in the struct.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ServiceDescriptor {
    /// Name of the k8s pod.
    pub k8s_pod_name: String,
    /// Name of the k8s namespace.
    pub k8s_namespace_name: String,
    /// Name of the k8s cluster.
    pub k8s_cluster_name: String,
    /// Name of the deployment environment.
    /// Note that the single deployment environment can be spread among multiple clusters.
    pub deployment_environment: String,
    /// Name of the service.
    pub service_name: String,
}

impl Default for ServiceDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceDescriptor {
    /// Environment variable to fetch the k8s pod name.
    pub const K8S_POD_NAME_ENV_VAR: &'static str = "POD_NAME";
    /// Environment variable to fetch the k8s namespace name.
    pub const K8S_NAMESPACE_NAME_ENV_VAR: &'static str = "POD_NAMESPACE";
    /// Environment variable to fetch the k8s cluster name.
    pub const K8S_CLUSTER_NAME_ENV_VAR: &'static str = "CLUSTER_NAME";
    /// Environment variable to fetch the deployment environment.
    pub const DEPLOYMENT_ENVIRONMENT_ENV_VAR: &'static str = "DEPLOYMENT_ENVIRONMENT";
    /// Environment variable to fetch the service name.
    pub const SERVICE_NAME_ENV_VAR: &'static str = "SERVICE_NAME";
    /// Default value for the k8s pod name.
    pub const DEFAULT_K8S_POD_NAME: &'static str = "zksync-0";
    /// Default value for the k8s namespace name.
    pub const DEFAULT_K8S_NAMESPACE_NAME: &'static str = "local";
    /// Default value for the k8s cluster name.
    pub const DEFAULT_K8S_CLUSTER_NAME: &'static str = "local";
    /// Default value for the deployment environment.
    pub const DEFAULT_DEPLOYMENT_ENVIRONMENT: &'static str = "local";
    /// Default value for the service name.
    pub const DEFAULT_SERVICE_NAME: &'static str = "zksync";

    /// Creates a filled `ServiceDescriptor` object.
    /// Fetched fields can be overridden.
    pub fn new() -> Self {
        // Attempt fetching data from environment variables, and use defaults if not provided.
        fn env_or(env_var: &str, default: &str) -> String {
            std::env::var(env_var).unwrap_or_else(|_| default.to_string())
        }
        Self {
            k8s_pod_name: env_or(Self::K8S_POD_NAME_ENV_VAR, Self::DEFAULT_K8S_POD_NAME),
            k8s_namespace_name: env_or(
                Self::K8S_NAMESPACE_NAME_ENV_VAR,
                Self::DEFAULT_K8S_NAMESPACE_NAME,
            ),
            k8s_cluster_name: env_or(
                Self::K8S_CLUSTER_NAME_ENV_VAR,
                Self::DEFAULT_K8S_CLUSTER_NAME,
            ),
            deployment_environment: env_or(
                Self::DEPLOYMENT_ENVIRONMENT_ENV_VAR,
                Self::DEFAULT_DEPLOYMENT_ENVIRONMENT,
            ),
            service_name: env_or(Self::SERVICE_NAME_ENV_VAR, Self::DEFAULT_SERVICE_NAME),
        }
    }

    pub fn with_k8s_pod_name(mut self, k8s_pod_name: Option<String>) -> Self {
        if let Some(k8s_pod_name) = k8s_pod_name {
            self.k8s_pod_name = k8s_pod_name;
        }
        self
    }

    pub fn with_k8s_namespace_name(mut self, k8s_namespace_name: Option<String>) -> Self {
        if let Some(k8s_namespace_name) = k8s_namespace_name {
            self.k8s_namespace_name = k8s_namespace_name;
        }
        self
    }

    pub fn with_service_name(mut self, service_name: Option<String>) -> Self {
        if let Some(service_name) = service_name {
            self.service_name = service_name;
        }
        self
    }

    fn into_otlp_resource(self) -> Resource {
        let attributes = vec![
            KeyValue::new(attribute::K8S_POD_NAME, self.k8s_pod_name),
            KeyValue::new(attribute::K8S_NAMESPACE_NAME, self.k8s_namespace_name),
            KeyValue::new(attribute::K8S_CLUSTER_NAME, self.k8s_cluster_name),
            #[allow(deprecated)]
            KeyValue::new(
                attribute::DEPLOYMENT_ENVIRONMENT,
                self.deployment_environment,
            ),
            KeyValue::new(attribute::SERVICE_NAME, self.service_name),
        ];
        Resource::builder_empty()
            .with_attributes(attributes)
            .build()
    }
}

#[derive(Debug)]
pub struct OpenTelemetry {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    pub opentelemetry_level: OpenTelemetryLevel,
    /// Opentelemetry HTTP collector endpoint for traces.
    pub tracing_endpoint: Option<Url>,
    /// Opentelemetry HTTP collector endpoint for logs.
    pub logging_endpoint: Option<Url>,
    /// Information about service
    pub service: ServiceDescriptor,
}

impl OpenTelemetry {
    pub fn new(
        opentelemetry_level: &str,
        tracing_endpoint: Option<String>,
        logging_endpoint: Option<String>,
    ) -> Result<Self, OpenTelemetryLayerError> {
        fn parse_url(url: Option<String>) -> Result<Option<Url>, OpenTelemetryLayerError> {
            url.map(|v| {
                v.parse()
                    .map_err(|e| OpenTelemetryLayerError::InvalidUrl(v, e))
            })
            .transpose()
        }

        Ok(Self {
            opentelemetry_level: opentelemetry_level.parse()?,
            tracing_endpoint: parse_url(tracing_endpoint)?,
            logging_endpoint: parse_url(logging_endpoint)?,
            service: ServiceDescriptor::new(),
        })
    }

    /// Can be used to override the service descriptor used by the layer.
    pub fn with_service_descriptor(mut self, service: ServiceDescriptor) -> Self {
        self.service = service;
        self
    }

    /// Prepares an exporter for OTLP logs and layer for the `tracing` library.
    /// Will return `None` if no logging URL was provided.
    ///
    /// *Important*: we use `tracing` library to generate logs, and convert the logs
    /// to OTLP format when exporting. However, `tracing` doesn't provide information
    /// about timestamp of the log. While this value is optional in OTLP, some
    /// collectors/processors may ignore logs without timestamp. Thus, you may need to
    /// have a proxy collector, like `opentelemetry-collector-contrib` or `vector`, and
    /// use the functionality there to set the timestamp. Here's example configuration
    /// for `opentelemetry-collector-contrib`:
    ///
    /// ```text
    /// processors:
    ///  transform/set_time_unix_nano:
    ///  log_statements:
    ///    - context: log
    ///      statements:
    ///        - set(time_unix_nano, observed_time_unix_nano)
    /// ```
    pub(super) fn logs_layer<S>(
        &self,
    ) -> Option<(opentelemetry_sdk::logs::SdkLoggerProvider, impl Layer<S>)>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        let logging_endpoint = self.logging_endpoint.clone()?;
        let resource = self.service.clone().into_otlp_resource();

        let exporter = opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_endpoint(logging_endpoint)
            .build()
            .expect("Failed to create OTLP exporter"); // URL is validated.

        let provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build();

        let layer =
            opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&provider);

        Some((provider, layer))
    }

    /// Prepares an exporter for OTLP traces and layer for `tracing` library.
    /// Will return `None` if no tracing URL was provided.
    pub(super) fn tracing_layer<S>(
        &self,
    ) -> Option<(opentelemetry_sdk::trace::SdkTracerProvider, impl Layer<S>)>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        let tracing_endpoint = self.tracing_endpoint.clone()?;
        // `otel::tracing` should be a level info to emit opentelemetry trace & span
        // `otel` set to debug to log detected resources, configuration read and inferred
        let filter = self
            .filter()
            .add_directive("otel::tracing=trace".parse().unwrap())
            .add_directive("otel=debug".parse().unwrap());

        let service_name = self.service.service_name.clone();
        let resource = self.service.clone().into_otlp_resource();

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(tracing_endpoint)
            .build()
            .expect("Failed to create OTLP exporter"); // URL is validated.

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_sampler(Sampler::AlwaysOn)
            .with_resource(resource)
            .build();

        // TODO: Version and other metadata
        let tracer = provider.tracer(service_name);

        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
        let layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(filter);

        Some((provider, layer))
    }

    /// Returns a filter for opentelemetry layer.
    /// It's applied to the layer only, but note that there might be a global filter applied to the
    /// whole subscriber.
    fn filter(&self) -> EnvFilter {
        match self.opentelemetry_level {
            OpenTelemetryLevel::OFF => EnvFilter::new("off"),
            OpenTelemetryLevel::INFO => EnvFilter::new("info"),
            OpenTelemetryLevel::DEBUG => EnvFilter::new("debug"),
            OpenTelemetryLevel::TRACE => EnvFilter::new("trace"),
        }
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
