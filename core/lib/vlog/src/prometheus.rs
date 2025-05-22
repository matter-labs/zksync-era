//! Prometheus-related functionality, such as [`PrometheusExporterConfig`].

use std::{env, net::Ipv4Addr, time::Duration};

use anyhow::Context as _;
use tokio::sync::watch;
use vise::MetricsCollection;
use vise_exporter::MetricsExporter;

#[derive(Debug, Clone)]
enum PrometheusTransport {
    Pull {
        port: u16,
    },
    Push {
        gateway_uri: String,
        interval: Duration,
    },
}

/// Configuration of a Prometheus exporter.
#[derive(Debug, Clone)]
pub struct PrometheusExporterConfig {
    transport: PrometheusTransport,
}

impl PrometheusExporterConfig {
    /// Creates an exporter that will run an HTTP server on the specified `port`.
    pub const fn pull(port: u16) -> Self {
        Self {
            transport: PrometheusTransport::Pull { port },
        }
    }

    /// Creates an exporter that will push metrics to the specified Prometheus gateway endpoint.
    pub const fn push(gateway_uri: String, interval: Duration) -> Self {
        Self {
            transport: PrometheusTransport::Push {
                gateway_uri,
                interval,
            },
        }
    }

    /// Creates a full push gateway endpoint.
    pub fn gateway_endpoint(base_url: &str) -> String {
        let job_id = "zksync-pushgateway";
        let namespace =
            env::var("POD_NAMESPACE").unwrap_or_else(|_| "UNKNOWN_NAMESPACE".to_owned());
        let pod = env::var("POD_NAME").unwrap_or_else(|_| "UNKNOWN_POD".to_owned());
        format!("{base_url}/metrics/job/{job_id}/namespace/{namespace}/pod/{pod}")
    }

    /// Runs the exporter. This future should be spawned in a separate Tokio task.
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let registry = MetricsCollection::lazy().collect();
        let metrics_exporter =
            MetricsExporter::new(registry.into()).with_graceful_shutdown(async move {
                stop_receiver.changed().await.ok();
            });

        match self.transport {
            PrometheusTransport::Pull { port } => {
                let prom_bind_address = (Ipv4Addr::UNSPECIFIED, port).into();
                metrics_exporter
                    .start(prom_bind_address)
                    .await
                    .context("Failed starting metrics server")?;
            }
            PrometheusTransport::Push {
                gateway_uri,
                interval,
            } => {
                let endpoint = gateway_uri
                    .parse()
                    .context("Failed parsing Prometheus push gateway endpoint")?;
                metrics_exporter.push_to_gateway(endpoint, interval).await;
            }
        }
        Ok(())
    }
}
