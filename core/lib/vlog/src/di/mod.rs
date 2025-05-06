//! Dependency injection for observability.

pub use self::{prometheus_exporter::PrometheusExporterLayer, sigint::SigintHandlerLayer};

mod prometheus_exporter;
mod sigint;
