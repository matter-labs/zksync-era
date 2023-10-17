use anyhow::Context as _;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};
use tokio::sync::watch;
use vise::MetricsCollection;
use vise_exporter::MetricsExporter;

use std::{net::Ipv4Addr, time::Duration};

fn configure_legacy_exporter(builder: PrometheusBuilder) -> PrometheusBuilder {
    // in seconds
    let default_latency_buckets = [0.001, 0.005, 0.025, 0.1, 0.25, 1.0, 5.0, 30.0, 120.0];
    let slow_latency_buckets = [
        0.33, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 180.0, 600.0, 1800.0, 3600.0,
    ];
    let prover_buckets = [
        1.0, 10.0, 20.0, 40.0, 60.0, 120.0, 240.0, 360.0, 600.0, 1800.0, 3600.0,
    ];

    builder
        .set_buckets(&default_latency_buckets)
        .unwrap()
        .set_buckets_for_metric(Matcher::Prefix("server.prover".to_owned()), &prover_buckets)
        .unwrap()
        .set_buckets_for_metric(
            Matcher::Prefix("server.witness_generator".to_owned()),
            &slow_latency_buckets,
        )
        .unwrap()
}

#[derive(Debug)]
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
#[derive(Debug)]
pub struct PrometheusExporterConfig {
    transport: PrometheusTransport,
    use_new_facade: bool,
}

impl PrometheusExporterConfig {
    /// Creates an exporter that will run an HTTP server on the specified `port`.
    pub const fn pull(port: u16) -> Self {
        Self {
            transport: PrometheusTransport::Pull { port },
            use_new_facade: true,
        }
    }

    /// Creates an exporter that will push metrics to the specified Prometheus gateway endpoint.
    pub const fn push(gateway_uri: String, interval: Duration) -> Self {
        Self {
            transport: PrometheusTransport::Push {
                gateway_uri,
                interval,
            },
            use_new_facade: true,
        }
    }

    /// Disables the new metrics façade (`vise`), which is on by default.
    #[must_use]
    pub fn without_new_facade(self) -> Self {
        Self {
            use_new_facade: false,
            transport: self.transport,
        }
    }

    /// Runs the exporter. This future should be spawned in a separate Tokio task.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        if self.use_new_facade {
            self.run_with_new_facade(stop_receiver)
                .await
                .context("run_with_new_facade()")
        } else {
            self.run_without_new_facade()
                .await
                .context("run_without_new_facade()")
        }
    }

    async fn run_with_new_facade(
        self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let registry = MetricsCollection::lazy().collect();
        let metrics_exporter = MetricsExporter::new(registry.into())
            .with_legacy_exporter(configure_legacy_exporter)
            .with_graceful_shutdown(async move {
                stop_receiver.changed().await.ok();
            });

        match self.transport {
            PrometheusTransport::Pull { port } => {
                let prom_bind_address = (Ipv4Addr::UNSPECIFIED, port).into();
                metrics_exporter
                    .start(prom_bind_address)
                    .await
                    .expect("Failed starting metrics server");
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

    async fn run_without_new_facade(self) -> anyhow::Result<()> {
        let builder = match self.transport {
            PrometheusTransport::Pull { port } => {
                let prom_bind_address = (Ipv4Addr::UNSPECIFIED, port);
                PrometheusBuilder::new().with_http_listener(prom_bind_address)
            }
            PrometheusTransport::Push {
                gateway_uri,
                interval,
            } => PrometheusBuilder::new()
                .with_push_gateway(gateway_uri, interval, None, None)
                .context("PrometheusBuilder::with_push_gateway()")?,
        };
        let builder = configure_legacy_exporter(builder);
        let (recorder, exporter) = builder.build().context("PrometheusBuilder::build()")?;
        metrics::set_boxed_recorder(Box::new(recorder))
            .context("failed to set metrics recorder")?;
        exporter.await.context("Prometheus exporter failed")
    }
}
